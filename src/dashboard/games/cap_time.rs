//! Time a regenerating resource spent at its cap, reconstructed from snapshots.
//!
//! Between two snapshots the resource is projected forward at its regeneration rate,
//! assuming the player did nothing. The projection is trusted only when it is
//! checked: the next snapshot must match it and must follow within `max_gap`.
//! Spending only delays the cap, so the time before the projected cap is known to be
//! below it even across a long gap or a detected spend; after that point, a long gap
//! or a mismatch makes the time unknown instead of guessed. After the last snapshot the
//! time before the projected cap stays below it (the same rule as above), and the
//! projected cap is trusted for `max_gap`; the cap is unknown beyond that.
use chrono::{DateTime, Duration, Utc};

/// One observation of the resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::dashboard) struct Sample {
    pub(in crate::dashboard) at: DateTime<Utc>,
    pub(in crate::dashboard) current: i64,
    pub(in crate::dashboard) maximum: i64,
    /// Upstream's full-recovery time, more precise than whole units.
    pub(in crate::dashboard) full_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::dashboard) enum CapState {
    AtCap,
    BelowCap,
    Unknown,
}

impl CapState {
    pub(in crate::dashboard) fn as_str(self) -> &'static str {
        match self {
            Self::AtCap => "at_cap",
            Self::BelowCap => "below_cap",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::dashboard) struct Segment {
    pub(in crate::dashboard) state: CapState,
    pub(in crate::dashboard) start: DateTime<Utc>,
    pub(in crate::dashboard) end: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::dashboard) struct CapTime {
    pub(in crate::dashboard) at_cap: Duration,
    pub(in crate::dashboard) unknown: Duration,
    /// Adjacent, non-empty, chronological and covering the whole window.
    pub(in crate::dashboard) segments: Vec<Segment>,
}

/// Rules of one reconstruction.
#[derive(Clone, Copy, Debug)]
pub(in crate::dashboard) struct Rules {
    /// Regeneration time of one unit.
    pub(in crate::dashboard) unit: Duration,
    /// The longest interval between snapshots that is still trusted.
    pub(in crate::dashboard) max_gap: Duration,
}

/// When `sample`'s resource reaches its cap if nothing happens. Upstream's full
/// time is used when it agrees with the count to within one unit.
fn cap_at(sample: &Sample, unit: Duration) -> DateTime<Utc> {
    if sample.current >= sample.maximum {
        return sample.at;
    }
    // Counts are bounded to 0..=100000 (DB CHECK and `sample_from_summary`), so the
    // difference fits in `i32`.
    let computed = sample.at + unit * (sample.maximum - sample.current) as i32;
    sample
        .full_at
        .filter(|full_at| *full_at >= sample.at && (*full_at - computed).abs() <= unit)
        .unwrap_or(computed)
}

/// The count `sample` projects for `at` if nothing happens.
fn projected(sample: &Sample, at: DateTime<Utc>, unit: Duration) -> i64 {
    let cap = cap_at(sample, unit);
    if at >= cap {
        return sample.current.max(sample.maximum);
    }
    let left = cap - at;
    let step = unit.num_milliseconds().max(1);
    // `left` is positive here: round the units still to regenerate up.
    let units_left = (left.num_milliseconds() + step - 1) / step;
    (sample.maximum - units_left).max(sample.current)
}

pub(in crate::dashboard) fn time_at_cap(
    samples: &[Sample],
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    rules: Rules,
) -> CapTime {
    let mut ordered: Vec<Sample> = samples
        .iter()
        .copied()
        .filter(|sample| sample.maximum > 0 && sample.current >= 0 && sample.at <= window_end)
        .collect();
    ordered.sort_by_key(|sample| sample.at);
    ordered.dedup_by_key(|sample| sample.at);

    let mut raw = Vec::new();
    if let Some(first) = ordered.first() {
        push(&mut raw, CapState::Unknown, window_start, first.at);
    } else {
        push(&mut raw, CapState::Unknown, window_start, window_end);
    }
    for pair in ordered.windows(2) {
        interval(&mut raw, &pair[0], Some(&pair[1]), pair[1].at, rules);
    }
    if let Some(last) = ordered.last() {
        interval(&mut raw, last, None, window_end, rules);
    }

    let mut segments: Vec<Segment> = Vec::new();
    for segment in raw {
        let start = segment.start.max(window_start);
        let end = segment.end.min(window_end);
        if start >= end {
            continue;
        }
        match segments.last_mut() {
            Some(previous) if previous.state == segment.state && previous.end == start => {
                previous.end = end;
            }
            _ => segments.push(Segment {
                state: segment.state,
                start,
                end,
            }),
        }
    }
    let total = |state| {
        segments
            .iter()
            .filter(|segment| segment.state == state)
            .fold(Duration::zero(), |sum, segment| {
                sum + (segment.end - segment.start)
            })
    };
    CapTime {
        at_cap: total(CapState::AtCap),
        unknown: total(CapState::Unknown),
        segments,
    }
}

fn push(out: &mut Vec<Segment>, state: CapState, start: DateTime<Utc>, end: DateTime<Utc>) {
    if start < end {
        out.push(Segment { state, start, end });
    }
}

/// The states from `from` until `end`, checked against `next` when there is one.
fn interval(
    out: &mut Vec<Segment>,
    from: &Sample,
    next: Option<&Sample>,
    end: DateTime<Utc>,
    rules: Rules,
) {
    let cap = cap_at(from, rules.unit).max(from.at);
    let reaches_cap = cap < end;
    let short = end - from.at <= rules.max_gap;
    // Within one unit of the projection: no spend or refill between the snapshots.
    let (matches, refilled) = match next {
        Some(next) => {
            let expected = projected(from, next.at, rules.unit);
            let same_cap = next.maximum == from.maximum;
            (
                same_cap && (next.current - expected).abs() <= 1,
                !same_cap || next.current > expected + 1,
            )
        }
        // After the last snapshot nothing can be checked; a short stretch is projected.
        None => (true, false),
    };
    if refilled {
        // An item refill (or a changed cap) at an unknown time: it may have reached the cap.
        push(out, CapState::Unknown, from.at, end);
    } else if !reaches_cap {
        // Spending only delays the cap, so the whole interval stayed below it.
        push(out, CapState::BelowCap, from.at, end);
    } else if matches && short {
        push(out, CapState::BelowCap, from.at, cap);
        push(out, CapState::AtCap, cap, end);
    } else if next.is_none() {
        // A long stretch after the last snapshot: projected only for the gap.
        let trusted = from.at + rules.max_gap;
        push(out, CapState::BelowCap, from.at, cap);
        push(out, CapState::AtCap, cap, trusted);
        push(out, CapState::Unknown, cap.max(trusted), end);
    } else {
        push(out, CapState::BelowCap, from.at, cap);
        push(out, CapState::Unknown, cap, end);
    }
}
