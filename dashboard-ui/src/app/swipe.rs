//! When a finished one-finger touch counts as a horizontal tab swipe, and which tab it
//! selects. Platform-free: the browser build (web-sys, `pages/swipe_tabs.rs`) and the app
//! (`mobile::swipe::SwipeListener`) only collect a [`Gesture`]; every threshold lives here.
use serde::Deserialize;

/// Shortest horizontal travel that switches tabs at any speed, in CSS pixels.
pub(crate) const MIN_DISTANCE: f64 = 50.0;
/// A faster flick switches from this travel on.
pub(crate) const FLICK_DISTANCE: f64 = 30.0;
/// Average speed of a flick, in CSS pixels per millisecond.
pub(crate) const FLICK_SPEED: f64 = 0.5;
/// Horizontal travel must exceed the vertical one by this factor, so a slightly slanted
/// vertical scroll never switches tabs.
pub(crate) const DOMINANCE: f64 = 1.5;
/// Touches starting this close to the left or right screen edge belong to the system:
/// iOS Safari's back/forward swipe and Android's back gesture.
pub(crate) const EDGE_GUARD: f64 = 24.0;
/// Elements whose touches never swipe: form controls (range sliders, text selection in
/// inputs), switches and anything a caller marks with `data-swipe-ignore`. Horizontally
/// scrollable elements are excluded separately by the collectors.
#[cfg(any(target_arch = "wasm32", feature = "mobile"))]
pub(crate) const IGNORE_SELECTOR: &str =
    "input,select,textarea,[contenteditable],[role=slider],[data-swipe-ignore]";

/// One finished touch as the collectors report it (CSS pixels, milliseconds).
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub(crate) struct Gesture {
    /// End minus start, horizontally; negative = the finger moved left.
    pub dx: f64,
    /// End minus start, vertically.
    pub dy: f64,
    /// Time between touchstart and touchend.
    pub ms: f64,
    /// Where the touch started, from the viewport's left edge.
    pub start_x: f64,
    /// Viewport width at touchstart.
    pub width: f64,
    /// The page scrolled during the touch: it was a vertical scroll.
    pub scrolled: bool,
    /// A second finger touched during the gesture (pinch zoom).
    pub multi_touch: bool,
}

/// Which neighbour a swipe asks for. Swiping left (content follows the finger to the
/// left) shows the next tab, swiping right the previous one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwipeDirection {
    Next,
    Previous,
}

impl SwipeDirection {
    /// CSS hook for the enter animation (`data-swipe-enter`).
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Previous => "previous",
        }
    }
}

/// The direction a finished touch asks for, or `None` when it was a tap, a scroll, a
/// pinch, an edge gesture or too short/slow.
pub(crate) fn classify(gesture: &Gesture) -> Option<SwipeDirection> {
    if gesture.scrolled || gesture.multi_touch {
        return None;
    }
    if gesture.width <= 2.0 * EDGE_GUARD
        || gesture.start_x < EDGE_GUARD
        || gesture.start_x > gesture.width - EDGE_GUARD
    {
        return None;
    }
    let horizontal = gesture.dx.abs();
    if !horizontal.is_finite() || horizontal <= gesture.dy.abs() * DOMINANCE {
        return None;
    }
    let speed = if gesture.ms > 0.0 {
        horizontal / gesture.ms
    } else {
        0.0
    };
    let far_enough =
        horizontal >= MIN_DISTANCE || (horizontal >= FLICK_DISTANCE && speed >= FLICK_SPEED);
    if !far_enough {
        return None;
    }
    Some(if gesture.dx < 0.0 {
        SwipeDirection::Next
    } else {
        SwipeDirection::Previous
    })
}

/// The tab index a swipe selects, or `None` at either end (no wrap-around).
pub(crate) fn step(index: usize, count: usize, direction: SwipeDirection) -> Option<usize> {
    match direction {
        SwipeDirection::Next => index.checked_add(1).filter(|next| *next < count),
        SwipeDirection::Previous if index < count => index.checked_sub(1),
        SwipeDirection::Previous => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swipe(dx: f64, dy: f64, ms: f64) -> Gesture {
        Gesture {
            dx,
            dy,
            ms,
            start_x: 200.0,
            width: 400.0,
            ..Gesture::default()
        }
    }

    #[test]
    fn left_swipe_selects_the_next_tab_and_right_swipe_the_previous_one() {
        assert_eq!(
            classify(&swipe(-60.0, 5.0, 400.0)),
            Some(SwipeDirection::Next)
        );
        assert_eq!(
            classify(&swipe(60.0, -5.0, 400.0)),
            Some(SwipeDirection::Previous)
        );
    }

    #[test]
    fn short_swipes_need_a_flick() {
        // 49px at any slow pace is not enough.
        assert_eq!(classify(&swipe(-49.0, 0.0, 300.0)), None);
        // 40px in 60ms (0.67px/ms) is a flick; 40px in 200ms (0.2px/ms) is not.
        assert_eq!(
            classify(&swipe(-40.0, 0.0, 60.0)),
            Some(SwipeDirection::Next)
        );
        assert_eq!(classify(&swipe(-40.0, 0.0, 200.0)), None);
        // Below the flick distance even a fast touch is a tap.
        assert_eq!(classify(&swipe(-29.0, 0.0, 10.0)), None);
        assert_eq!(classify(&swipe(0.0, 0.0, 0.0)), None);
    }

    #[test]
    fn vertical_or_slanted_scrolls_never_switch() {
        assert_eq!(classify(&swipe(-80.0, 200.0, 300.0)), None);
        // Mostly horizontal but within the dominance factor: still treated as a scroll.
        assert_eq!(classify(&swipe(-90.0, 60.0, 300.0)), None);
        assert_eq!(
            classify(&swipe(-91.0, 60.0, 300.0)),
            Some(SwipeDirection::Next)
        );
        let scrolled = Gesture {
            scrolled: true,
            ..swipe(-120.0, 0.0, 200.0)
        };
        assert_eq!(classify(&scrolled), None);
    }

    #[test]
    fn pinches_and_edge_gestures_are_left_to_the_system() {
        let pinch = Gesture {
            multi_touch: true,
            ..swipe(-120.0, 0.0, 200.0)
        };
        assert_eq!(classify(&pinch), None);
        for start_x in [0.0, 23.9, 376.1, 400.0] {
            let edge = Gesture {
                start_x,
                ..swipe(120.0, 0.0, 200.0)
            };
            assert_eq!(classify(&edge), None, "start_x {start_x}");
        }
        for start_x in [24.0, 376.0] {
            let inside = Gesture {
                start_x,
                ..swipe(-120.0, 0.0, 200.0)
            };
            assert_eq!(classify(&inside), Some(SwipeDirection::Next));
        }
        let unknown_width = Gesture {
            width: 0.0,
            ..swipe(-120.0, 0.0, 200.0)
        };
        assert_eq!(classify(&unknown_width), None);
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
    fn collectors_report_gestures_as_snake_case_json() {
        let gesture: Gesture = serde_json::from_str(
            r#"{"dx":-60,"dy":4,"ms":180,"start_x":120,"width":390,"scrolled":false,"multi_touch":false}"#,
        )
        .expect("gesture JSON");
        assert_eq!(classify(&gesture), Some(SwipeDirection::Next));
        assert_eq!(SwipeDirection::Next.name(), "next");
        assert_eq!(SwipeDirection::Previous.name(), "previous");
    }
}
