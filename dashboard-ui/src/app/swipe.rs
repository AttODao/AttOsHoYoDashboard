//! The tab pager's policy: when a touch becomes a horizontal pager drag, how far the panels
//! follow the finger, whether a release completes the swipe, and where and how fast the
//! panels settle. Platform-free: the browser build (web-sys, `pages/swipe_tabs.rs`) and the
//! app (`mobile::swipe::SwipeListener`) only measure touches and move the panels; every
//! threshold lives here. The app's WebView script runs [`starts_inside`], [`lock`] and
//! [`drag_offset`] per frame without a round trip, so it receives [`POLICY`] (these
//! constants) and mirrors those three formulas; it asks [`settle`] once per release.
// Native test builds have no collector; the policy is exercised by the tests below.
#![cfg_attr(not(any(target_arch = "wasm32", feature = "mobile")), allow(dead_code))]
use serde::{Deserialize, Serialize};

/// Shortest horizontal travel that completes a swipe at any speed, in CSS pixels.
pub(crate) const MIN_DISTANCE: f64 = 50.0;
/// A faster flick completes from this travel on.
pub(crate) const FLICK_DISTANCE: f64 = 30.0;
/// Average speed of a flick, in CSS pixels per millisecond. Moving back toward the start
/// at least this fast when the finger lifts cancels the swipe.
pub(crate) const FLICK_SPEED: f64 = 0.5;
/// Horizontal travel must exceed the vertical one by this factor before the panels move,
/// so a slightly slanted vertical scroll never drags the pager.
pub(crate) const DOMINANCE: f64 = 1.5;
/// The finger moves this far before the direction is decided (pager or page scroll). At or
/// below Chromium's touch slop (8dp on Android), so the pager can claim the touch before
/// the browser starts its own gesture.
pub(crate) const SLOP: f64 = 8.0;
/// Touches starting this close to the left or right screen edge belong to the system:
/// iOS Safari's back/forward swipe and Android's back gesture.
pub(crate) const EDGE_GUARD: f64 = 24.0;
/// Space between the current panel and the neighbour beside it while dragging.
pub(crate) const PAGE_GAP: f64 = 16.0;
/// At the first and last tab the panel follows the finger at this fraction ...
pub(crate) const EDGE_RESISTANCE: f64 = 0.35;
/// ... and never further than this (rubber band).
pub(crate) const MAX_OVERSCROLL: f64 = 56.0;
/// Settle animations take this long for a whole page width, and at least `MIN_SETTLE_MS`.
pub(crate) const SETTLE_MS: f64 = 280.0;
pub(crate) const MIN_SETTLE_MS: f64 = 140.0;
/// The release speed is measured over the last moves within this window.
pub(crate) const VELOCITY_WINDOW_MS: f64 = 100.0;
/// The settle curve: an exponential ease-out from where the finger let go.
pub(crate) const SETTLE_EASING: &str = "cubic-bezier(.16,1,.3,1)";
/// Elements whose touches never swipe: form controls (range sliders, text selection in
/// inputs), switches and anything a caller marks with `data-swipe-ignore`. Horizontally
/// scrollable elements are excluded separately by the collectors.
#[cfg(any(target_arch = "wasm32", feature = "mobile"))]
pub(crate) const IGNORE_SELECTOR: &str =
    "input,select,textarea,[contenteditable],[role=slider],[data-swipe-ignore]";

/// The constants the app's per-frame script needs, as one JSON object.
#[cfg_attr(not(feature = "mobile"), allow(dead_code))]
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Policy {
    pub slop: f64,
    pub dominance: f64,
    pub edge: f64,
    pub gap: f64,
    pub resistance: f64,
    pub overscroll: f64,
    pub window: f64,
    pub easing: &'static str,
}

#[cfg_attr(not(feature = "mobile"), allow(dead_code))]
pub(crate) const POLICY: Policy = Policy {
    slop: SLOP,
    dominance: DOMINANCE,
    edge: EDGE_GUARD,
    gap: PAGE_GAP,
    resistance: EDGE_RESISTANCE,
    overscroll: MAX_OVERSCROLL,
    window: VELOCITY_WINDOW_MS,
    easing: SETTLE_EASING,
};

/// Whether a touch at `start_x` may drag the pager at all: not at a screen edge.
#[cfg_attr(
    feature = "mobile",
    allow(dead_code, reason = "mirrored by the app script")
)]
pub(crate) fn starts_inside(start_x: f64, width: f64) -> bool {
    width > 2.0 * EDGE_GUARD && start_x >= EDGE_GUARD && start_x <= width - EDGE_GUARD
}

/// What a moving touch is, once decided.
#[cfg_attr(
    feature = "mobile",
    allow(dead_code, reason = "mirrored by the app script")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lock {
    /// Still within the slop.
    Undecided,
    /// The panels follow the finger; the page does not scroll (`touch-action: pan-y`).
    Pager,
    /// A page scroll (or anything not clearly horizontal): the panels never move.
    Scroll,
}

/// Decides the direction from the travel since touchstart. A page that already scrolled
/// is a scroll whatever the travel.
#[cfg_attr(
    feature = "mobile",
    allow(dead_code, reason = "mirrored by the app script")
)]
pub(crate) fn lock(dx: f64, dy: f64, scrolled: bool) -> Lock {
    if scrolled || !dx.is_finite() || !dy.is_finite() {
        return Lock::Scroll;
    }
    if dx.hypot(dy) < SLOP {
        return Lock::Undecided;
    }
    if dx.abs() > dy.abs() * DOMINANCE {
        Lock::Pager
    } else {
        Lock::Scroll
    }
}

/// Which neighbour a swipe asks for. Swiping left (content follows the finger to the
/// left) shows the next tab, swiping right the previous one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwipeDirection {
    Next,
    Previous,
}

impl SwipeDirection {
    fn of(dx: f64) -> Option<Self> {
        if dx < 0.0 {
            Some(Self::Next)
        } else if dx > 0.0 {
            Some(Self::Previous)
        } else {
            None
        }
    }
}

/// The tab index a swipe selects, or `None` at either end (no wrap-around).
pub(crate) fn step(index: usize, count: usize, direction: SwipeDirection) -> Option<usize> {
    match direction {
        SwipeDirection::Next => index.checked_add(1).filter(|next| *next < count),
        SwipeDirection::Previous if index < count => index.checked_sub(1),
        SwipeDirection::Previous => None,
    }
}

/// The neighbour shown beside the current panel for a drag of `dx`, if there is one.
pub(crate) fn peek(dx: f64, index: usize, count: usize) -> Option<usize> {
    step(index, count, SwipeDirection::of(dx)?)
}

/// How far the current panel moves for a drag of `dx`: with the finger toward an existing
/// neighbour, against a growing resistance (never past [`MAX_OVERSCROLL`]) at either end.
#[cfg_attr(
    feature = "mobile",
    allow(dead_code, reason = "mirrored by the app script")
)]
pub(crate) fn drag_offset(dx: f64, index: usize, count: usize) -> f64 {
    if !dx.is_finite() || dx == 0.0 {
        return 0.0;
    }
    if peek(dx, index, count).is_some() {
        return dx;
    }
    let pulled = dx.abs() * EDGE_RESISTANCE / MAX_OVERSCROLL;
    dx.signum() * MAX_OVERSCROLL * (1.0 - 1.0 / (pulled + 1.0))
}

/// Where a neighbour sits for the current panel's `offset`: one page width plus the gap to
/// the right (next) or left (previous).
#[cfg_attr(
    feature = "mobile",
    allow(dead_code, reason = "mirrored by the app script")
)]
pub(crate) fn neighbour_offset(
    offset: f64,
    index: usize,
    neighbour: usize,
    page_width: f64,
) -> f64 {
    let span = page_width + PAGE_GAP;
    if neighbour > index {
        offset + span
    } else {
        offset - span
    }
}

/// One finished pager drag as the collectors report it (CSS pixels, milliseconds).
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub(crate) struct Release {
    /// End minus start, horizontally; negative = the finger moved left.
    pub dx: f64,
    /// Time between touchstart and the release.
    pub ms: f64,
    /// Horizontal speed over the last moves (px/ms, signed like `dx`).
    pub velocity: f64,
    /// The touch ended with `touchcancel` (the system took it over).
    pub cancelled: bool,
    /// A second finger touched during the drag (pinch zoom).
    pub multi_touch: bool,
    /// The page scrolled during the drag.
    pub scrolled: bool,
    /// The current panel's drawn offset when the finger lifted.
    pub offset: f64,
    /// The pager's width.
    pub page_width: f64,
    /// `prefers-reduced-motion: reduce`.
    pub reduced: bool,
}

/// The tab a release settles on: the neighbour when the swipe completes, `None` when it
/// snaps back. The distance and flick thresholds of the finished-touch swipes stay the
/// minimum; seeing the position adds one rule, flinging back toward the start cancels.
/// No "past half the width" rule: 50px already completes, so it could only make short
/// slow drags (which switch tabs today) snap back.
pub(crate) fn release(release: &Release, index: usize, count: usize) -> Option<usize> {
    if release.cancelled || release.multi_touch || release.scrolled {
        return None;
    }
    let travel = release.dx.abs();
    if !travel.is_finite() {
        return None;
    }
    let speed = if release.ms > 0.0 {
        travel / release.ms
    } else {
        0.0
    };
    let far_enough = travel >= MIN_DISTANCE || (travel >= FLICK_DISTANCE && speed >= FLICK_SPEED);
    let flung_back = release.velocity * release.dx < 0.0 && release.velocity.abs() >= FLICK_SPEED;
    if !far_enough || flung_back {
        return None;
    }
    peek(release.dx, index, count)
}

/// Where the panels settle after a release and how long the move takes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(crate) struct Settle {
    /// The tab selected once the panels arrive; `None` snaps back.
    pub target: Option<usize>,
    /// The current panel's final offset (the neighbour sits beside it).
    pub offset: f64,
    /// Animation length; 0 with reduced motion or when already there.
    pub ms: f64,
}

pub(crate) fn settle(drag: &Release, index: usize, count: usize) -> Settle {
    let target = release(drag, index, count);
    let width = if drag.page_width.is_finite() {
        drag.page_width.max(0.0)
    } else {
        0.0
    };
    let offset = match target {
        Some(next) if next > index => -(width + PAGE_GAP),
        Some(_) => width + PAGE_GAP,
        None => 0.0,
    };
    Settle {
        target,
        offset,
        ms: settle_ms(drag.offset, offset, width, drag.reduced),
    }
}

/// The settle length: proportional to the remaining distance, between [`MIN_SETTLE_MS`]
/// and [`SETTLE_MS`]. Reduced motion settles at once (the drag itself still follows the
/// finger: that is direct manipulation, not animation).
pub(crate) fn settle_ms(from: f64, to: f64, page_width: f64, reduced: bool) -> f64 {
    let distance = (to - from).abs();
    if reduced || !distance.is_finite() || distance < 0.5 {
        return 0.0;
    }
    let share = (distance / (page_width + PAGE_GAP).max(1.0)).min(1.0);
    (MIN_SETTLE_MS + (SETTLE_MS - MIN_SETTLE_MS) * share).round()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(dx: f64, ms: f64) -> Release {
        Release {
            dx,
            ms,
            velocity: dx / ms,
            offset: dx,
            page_width: 320.0,
            ..Release::default()
        }
    }

    #[test]
    fn the_direction_waits_for_the_slop_then_locks() {
        assert_eq!(lock(0.0, 0.0, false), Lock::Undecided);
        assert_eq!(lock(-5.0, 5.0, false), Lock::Undecided);
        assert_eq!(lock(-12.0, 2.0, false), Lock::Pager);
        assert_eq!(lock(12.0, -2.0, false), Lock::Pager);
        // Mostly horizontal but within the dominance factor: a scroll, never the pager.
        assert_eq!(lock(-15.0, 10.0, false), Lock::Scroll);
        assert_eq!(lock(-15.1, 10.0, false), Lock::Pager);
        assert_eq!(lock(2.0, 30.0, false), Lock::Scroll);
        // A page that scrolled is a scroll, even inside the slop.
        assert_eq!(lock(-3.0, 0.0, true), Lock::Scroll);
        assert_eq!(lock(-40.0, 0.0, true), Lock::Scroll);
        assert_eq!(lock(f64::NAN, 0.0, false), Lock::Scroll);
    }

    #[test]
    fn edge_starts_are_left_to_the_system() {
        for start_x in [0.0, 23.9, 376.1, 400.0] {
            assert!(!starts_inside(start_x, 400.0), "start_x {start_x}");
        }
        for start_x in [24.0, 200.0, 376.0] {
            assert!(starts_inside(start_x, 400.0), "start_x {start_x}");
        }
        assert!(!starts_inside(20.0, 0.0));
        assert!(!starts_inside(24.0, 48.0));
    }

    #[test]
    fn the_panel_follows_the_finger_toward_a_neighbour() {
        assert_eq!(drag_offset(-120.0, 0, 2), -120.0);
        assert_eq!(drag_offset(80.0, 1, 2), 80.0);
        assert_eq!(drag_offset(0.0, 0, 2), 0.0);
        assert_eq!(peek(-1.0, 0, 2), Some(1));
        assert_eq!(peek(1.0, 1, 2), Some(0));
        assert_eq!(peek(0.0, 1, 3), None);
        assert_eq!(neighbour_offset(-100.0, 0, 1, 320.0), 236.0);
        assert_eq!(neighbour_offset(100.0, 1, 0, 320.0), -236.0);
    }

    #[test]
    fn the_ends_resist_like_a_rubber_band() {
        assert_eq!(peek(10.0, 0, 3), None);
        assert_eq!(peek(-10.0, 2, 3), None);
        let small = drag_offset(20.0, 0, 3);
        let large = drag_offset(200.0, 0, 3);
        let huge = drag_offset(5000.0, 0, 3);
        assert!(small > 0.0 && small < 20.0 * EDGE_RESISTANCE, "{small}");
        assert!(large > small && large < 200.0 * EDGE_RESISTANCE, "{large}");
        assert!(huge > large && huge < MAX_OVERSCROLL, "{huge}");
        assert_eq!(drag_offset(-200.0, 2, 3), -large);
        assert_eq!(drag_offset(f64::INFINITY, 0, 3), 0.0);
    }

    #[test]
    fn releases_keep_the_distance_and_flick_minimums() {
        assert_eq!(release(&drag(-60.0, 400.0), 0, 2), Some(1));
        assert_eq!(release(&drag(60.0, 400.0), 1, 2), Some(0));
        // 49px at a slow pace is not enough.
        assert_eq!(release(&drag(-49.0, 300.0), 0, 2), None);
        // 40px in 60ms (0.67px/ms) is a flick; 40px in 200ms (0.2px/ms) is not.
        assert_eq!(release(&drag(-40.0, 60.0), 0, 2), Some(1));
        assert_eq!(release(&drag(-40.0, 200.0), 0, 2), None);
        // Below the flick distance even a fast touch snaps back.
        assert_eq!(release(&drag(-29.0, 10.0), 0, 2), None);
        assert_eq!(release(&drag(0.0, 0.0), 0, 2), None);
        // A long slow drag still completes: no half-width rule.
        assert_eq!(release(&drag(-150.0, 2000.0), 0, 3), Some(1));
    }

    #[test]
    fn flinging_back_cancels_and_ends_stay() {
        let back = Release {
            velocity: 0.6,
            ..drag(-150.0, 600.0)
        };
        assert_eq!(release(&back, 0, 2), None);
        let slow_back = Release {
            velocity: 0.2,
            ..drag(-150.0, 600.0)
        };
        assert_eq!(release(&slow_back, 0, 2), Some(1));
        assert_eq!(release(&drag(120.0, 200.0), 0, 2), None);
        assert_eq!(release(&drag(-120.0, 200.0), 1, 2), None);
        for broken in [
            Release {
                cancelled: true,
                ..drag(-120.0, 200.0)
            },
            Release {
                multi_touch: true,
                ..drag(-120.0, 200.0)
            },
            Release {
                scrolled: true,
                ..drag(-120.0, 200.0)
            },
        ] {
            assert_eq!(release(&broken, 0, 2), None, "{broken:?}");
        }
    }

    #[test]
    fn settling_moves_from_the_released_position() {
        let done = settle(&drag(-120.0, 200.0), 0, 2);
        assert_eq!(done.target, Some(1));
        assert_eq!(done.offset, -336.0);
        assert!(done.ms >= MIN_SETTLE_MS && done.ms <= SETTLE_MS, "{done:?}");
        let back = settle(&drag(120.0, 200.0), 1, 2);
        assert_eq!((back.target, back.offset), (Some(0), 336.0));
        // A short drag snaps back from where it was, quickly.
        let snap = settle(&drag(-20.0, 400.0), 0, 2);
        assert_eq!((snap.target, snap.offset), (None, 0.0));
        assert_eq!(snap.ms, MIN_SETTLE_MS + 8.0);
        // A longer way takes longer, but never more than SETTLE_MS.
        assert!(settle_ms(-20.0, -336.0, 320.0, false) > settle_ms(-200.0, -336.0, 320.0, false));
        assert_eq!(settle_ms(400.0, -336.0, 320.0, false), SETTLE_MS);
        assert_eq!(settle_ms(-336.0, -336.0, 320.0, false), 0.0);
    }

    #[test]
    fn reduced_motion_settles_at_once() {
        let reduced = Release {
            reduced: true,
            ..drag(-120.0, 200.0)
        };
        let done = settle(&reduced, 0, 2);
        assert_eq!((done.target, done.offset, done.ms), (Some(1), -336.0, 0.0));
        assert_eq!(settle_ms(10.0, 0.0, 320.0, true), 0.0);
    }

    #[test]
    fn steps_stop_at_both_ends() {
        assert_eq!(step(0, 3, SwipeDirection::Next), Some(1));
        assert_eq!(step(1, 3, SwipeDirection::Next), Some(2));
        assert_eq!(step(2, 3, SwipeDirection::Next), None);
        assert_eq!(step(2, 3, SwipeDirection::Previous), Some(1));
        assert_eq!(step(0, 3, SwipeDirection::Previous), None);
        assert_eq!(step(0, 0, SwipeDirection::Next), None);
        assert_eq!(step(5, 3, SwipeDirection::Previous), None);
        assert_eq!(step(usize::MAX, usize::MAX, SwipeDirection::Next), None);
    }

    #[test]
    fn collectors_exchange_snake_case_json() {
        let drag: Release = serde_json::from_str(
            r#"{"dx":-60,"ms":180,"velocity":-0.4,"cancelled":false,"multi_touch":false,"scrolled":false,"offset":-60,"page_width":320,"reduced":false}"#,
        )
        .expect("release JSON");
        assert_eq!(release(&drag, 0, 2), Some(1));
        let settled = serde_json::to_value(settle(&drag, 0, 2)).expect("settle JSON");
        assert_eq!(settled["target"], 1);
        assert_eq!(settled["offset"], -336.0);
        let snapped = serde_json::to_value(settle(&drag, 1, 2)).expect("settle JSON");
        assert!(snapped["target"].is_null());
        let policy = serde_json::to_value(POLICY).expect("policy JSON");
        assert_eq!(policy["slop"], SLOP);
        assert_eq!(policy["easing"], SETTLE_EASING);
    }
}
