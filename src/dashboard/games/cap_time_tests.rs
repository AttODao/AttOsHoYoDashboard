use super::cap_time::{CapState, CapTime, Rules, Sample, time_at_cap};
use chrono::{DateTime, Duration, TimeZone, Utc};

const RESIN: Rules = Rules {
    unit: Duration::minutes(8),
    max_gap: Duration::minutes(90),
};

fn t(minutes: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 2, 0, 0, 0).unwrap() + Duration::minutes(minutes)
}

fn sample(minutes: i64, current: i64) -> Sample {
    Sample {
        at: t(minutes),
        current,
        maximum: 200,
        full_at: None,
    }
}

fn states(result: &CapTime) -> Vec<(CapState, i64, i64)> {
    let origin = t(0);
    result
        .segments
        .iter()
        .map(|segment| {
            (
                segment.state,
                (segment.start - origin).num_minutes(),
                (segment.end - origin).num_minutes(),
            )
        })
        .collect()
}

#[test]
fn no_snapshots_leave_the_whole_window_unknown() {
    let result = time_at_cap(&[], t(0), t(600), RESIN);
    assert_eq!(result.at_cap, Duration::zero());
    assert_eq!(result.unknown, Duration::minutes(600));
    assert_eq!(states(&result), [(CapState::Unknown, 0, 600)]);
}

#[test]
fn checked_snapshots_count_the_time_after_the_projected_cap() {
    // 195 resin at 0 reaches 200 after 5 units (40 min); the next snapshots agree.
    let samples = [sample(0, 195), sample(60, 200), sample(120, 200)];
    let result = time_at_cap(&samples, t(0), t(120), RESIN);
    assert_eq!(result.at_cap, Duration::minutes(80));
    assert_eq!(result.unknown, Duration::zero());
    assert_eq!(
        states(&result),
        [(CapState::BelowCap, 0, 40), (CapState::AtCap, 40, 120)]
    );
}

#[test]
fn upstream_full_time_refines_the_cap_when_it_agrees_with_the_count() {
    let mut first = sample(0, 195);
    first.full_at = Some(t(35));
    let result = time_at_cap(&[first, sample(60, 200)], t(0), t(60), RESIN);
    assert_eq!(result.at_cap, Duration::minutes(25));
    // A full time far from the count is ignored.
    first.full_at = Some(t(300));
    let result = time_at_cap(&[first, sample(60, 200)], t(0), t(60), RESIN);
    assert_eq!(result.at_cap, Duration::minutes(20));
}

#[test]
fn a_long_gap_after_the_cap_is_unknown_but_the_time_before_it_is_not() {
    // 190 resin reaches the cap at 80 min; the next snapshot is 5 hours later.
    let samples = [sample(0, 190), sample(300, 200)];
    let result = time_at_cap(&samples, t(0), t(300), RESIN);
    assert_eq!(result.at_cap, Duration::zero());
    assert_eq!(result.unknown, Duration::minutes(220));
    assert_eq!(
        states(&result),
        [(CapState::BelowCap, 0, 80), (CapState::Unknown, 80, 300)]
    );
}

#[test]
fn a_long_gap_that_cannot_reach_the_cap_is_known_to_be_below_it() {
    // 20 resin needs 24 hours to fill; a spend in between only delays that.
    let samples = [sample(0, 20), sample(600, 40)];
    let result = time_at_cap(&samples, t(0), t(600), RESIN);
    assert_eq!(result.unknown, Duration::zero());
    assert_eq!(states(&result), [(CapState::BelowCap, 0, 600)]);
}

#[test]
fn a_spend_after_reaching_the_cap_makes_the_capped_part_unknown() {
    // At the cap at 0, but 40 resin were spent at some point before 60.
    let samples = [sample(0, 200), sample(60, 160)];
    let result = time_at_cap(&samples, t(0), t(60), RESIN);
    assert_eq!(result.at_cap, Duration::zero());
    assert_eq!(states(&result), [(CapState::Unknown, 0, 60)]);
}

#[test]
fn a_refill_or_a_changed_cap_makes_the_interval_unknown() {
    // 100 resin cannot regenerate to 180 in an hour: an item was used.
    let samples = [sample(0, 100), sample(60, 180)];
    let result = time_at_cap(&samples, t(0), t(60), RESIN);
    assert_eq!(states(&result), [(CapState::Unknown, 0, 60)]);
    let mut raised = sample(60, 200);
    raised.maximum = 240;
    let result = time_at_cap(&[sample(0, 200), raised], t(0), t(60), RESIN);
    assert_eq!(states(&result), [(CapState::Unknown, 0, 60)]);
}

#[test]
fn over_the_cap_counts_as_capped_while_it_holds() {
    let samples = [sample(0, 210), sample(60, 210)];
    let result = time_at_cap(&samples, t(0), t(60), RESIN);
    assert_eq!(result.at_cap, Duration::minutes(60));
}

#[test]
fn the_time_after_the_last_snapshot_is_projected_only_up_to_the_gap() {
    let result = time_at_cap(&[sample(0, 200)], t(0), t(60), RESIN);
    assert_eq!(states(&result), [(CapState::AtCap, 0, 60)]);
    let result = time_at_cap(&[sample(0, 200)], t(0), t(240), RESIN);
    assert_eq!(
        states(&result),
        [(CapState::AtCap, 0, 90), (CapState::Unknown, 90, 240)]
    );
    // Reaching the cap only after the trusted stretch leaves it unknown from there.
    let result = time_at_cap(&[sample(0, 185)], t(0), t(240), RESIN);
    assert_eq!(
        states(&result),
        [(CapState::BelowCap, 0, 120), (CapState::Unknown, 120, 240)]
    );
    let result = time_at_cap(&[sample(0, 100)], t(0), t(240), RESIN);
    assert_eq!(states(&result), [(CapState::BelowCap, 0, 240)]);
}

#[test]
fn the_window_clips_older_snapshots_and_leaves_the_start_unknown_without_one() {
    // A snapshot before the window anchors its start.
    let samples = [sample(-30, 200), sample(30, 200)];
    let result = time_at_cap(&samples, t(0), t(60), RESIN);
    assert_eq!(states(&result), [(CapState::AtCap, 0, 60)]);
    assert_eq!(result.at_cap, Duration::minutes(60));
    // Without one, the time before the first snapshot is unknown.
    let result = time_at_cap(&[sample(30, 200)], t(0), t(60), RESIN);
    assert_eq!(
        states(&result),
        [(CapState::Unknown, 0, 30), (CapState::AtCap, 30, 60)]
    );
}

#[test]
fn segments_are_merged_ordered_and_cover_the_window_whatever_the_input_order() {
    let samples = [
        sample(120, 200),
        sample(0, 200),
        sample(60, 200),
        sample(60, 200),
        Sample {
            maximum: 0,
            ..sample(90, 0)
        },
        sample(500, 0),
    ];
    let result = time_at_cap(&samples, t(0), t(180), RESIN);
    assert_eq!(states(&result), [(CapState::AtCap, 0, 180)]);
    let covered = result
        .segments
        .iter()
        .fold(Duration::zero(), |sum, segment| {
            sum + (segment.end - segment.start)
        });
    assert_eq!(covered, Duration::minutes(180));
}

#[test]
fn trailblaze_power_uses_its_own_rate() {
    let power = Rules {
        unit: Duration::minutes(6),
        max_gap: Duration::minutes(90),
    };
    let samples = [
        Sample {
            at: t(0),
            current: 290,
            maximum: 300,
            full_at: None,
        },
        Sample {
            at: t(80),
            current: 300,
            maximum: 300,
            full_at: None,
        },
    ];
    let result = time_at_cap(&samples, t(0), t(80), power);
    assert_eq!(result.at_cap, Duration::minutes(20));
}

#[test]
fn state_names_match_the_api() {
    assert_eq!(CapState::AtCap.as_str(), "at_cap");
    assert_eq!(CapState::BelowCap.as_str(), "below_cap");
    assert_eq!(CapState::Unknown.as_str(), "unknown");
}
