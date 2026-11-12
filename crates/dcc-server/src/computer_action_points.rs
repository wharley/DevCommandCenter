//! Where each task's agent last acted on the screen, for the agent's drawn
//! cursor: the point of a click, scroll, drag or set value; keys stay where
//! the agent last pointed, or the window's center when it never did.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A point in screen coordinates (points, top-left origin).
pub type ScreenPoint = (f64, f64);

#[derive(Clone, Default)]
pub struct ActionPoints {
    points: Arc<Mutex<HashMap<String, ScreenPoint>>>,
}

impl ActionPoints {
    /// After an action: `at` where it happened, or the window's `center`.
    /// A keyboard action keeps the point the agent already had.
    pub fn note(&self, session_id: &str, at: Option<ScreenPoint>, center: ScreenPoint, keys: bool) {
        let Ok(mut points) = self.points.lock() else {
            return;
        };
        if keys {
            points.entry(session_id.to_string()).or_insert(center);
        } else {
            points.insert(session_id.to_string(), at.unwrap_or(center));
        }
    }

    pub fn get(&self, session_id: &str) -> Option<ScreenPoint> {
        self.points.lock().ok()?.get(session_id).copied()
    }

    pub fn forget(&self, session_id: &str) {
        if let Ok(mut points) = self.points.lock() {
            points.remove(session_id);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut points) = self.points.lock() {
            points.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_stay_where_the_agent_pointed_and_start_at_the_window_center() {
        let points = ActionPoints::default();
        points.note("s", None, (50.0, 40.0), true);
        assert_eq!(points.get("s"), Some((50.0, 40.0)));
        points.note("s", Some((10.0, 20.0)), (50.0, 40.0), false);
        points.note("s", None, (50.0, 40.0), true);
        assert_eq!(points.get("s"), Some((10.0, 20.0)));
        points.forget("s");
        assert_eq!(points.get("s"), None);
    }
}
