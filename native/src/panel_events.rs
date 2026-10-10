//! Closed-schema UI events: never log arbitrary labels, tokens or exception text.
use crate::logging::RunLog;
use serde_json::Value;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Events {
    arrivals: VecDeque<Instant>,
}
impl Events {
    pub fn record(&mut self, payload: &Value, log: &RunLog) -> Result<u16, String> {
        let object = payload.as_object().ok_or("expected UI events object")?;
        let events = payload
            .get("events")
            .and_then(Value::as_array)
            .ok_or("expected 1 to32 UI events")?;
        if object.len() != 1 || events.is_empty() || events.len() > 32 {
            return Err("expected 1 to32 UI events".into());
        }
        let allowed = [
            "ready",
            "hold",
            "unhold",
            "undo",
            "redo",
            "reset",
            "selection",
            "reorder",
            "save_requested",
            "save_succeeded",
            "save_failed",
            "animation",
            "zoom",
            "backdrop",
            "error",
        ];
        let numbers = [
            "count",
            "revision",
            "status",
            "duration_ms",
            "line",
            "column",
            "zoom",
        ];
        let sources = [
            "panel-grid.js",
            "panel-motion.js",
            "panel-save.js",
            "panel-drag.js",
            "panel-actions.js",
            "panel-holding.js",
            "panel-draft.js",
            "window",
            "promise",
        ];
        for event in events {
            let row = event.as_object().ok_or("unknown UI event fields")?;
            if row.keys().any(|k| {
                !numbers.contains(&k.as_str()) && !["event", "source", "name"].contains(&k.as_str())
            }) {
                return Err("unknown UI event fields".into());
            }
            if !event
                .get("event")
                .and_then(Value::as_str)
                .is_some_and(|v| allowed.contains(&v))
            {
                return Err("unknown UI event".into());
            }
            for k in numbers {
                if let Some(v) = row.get(k)
                    && !v
                        .as_f64()
                        .is_some_and(|n| n.is_finite() && (0.0..=1e9).contains(&n))
                {
                    return Err("invalid UI event number".into());
                }
            }
            if let Some(v) = row.get("source")
                && !v.as_str().is_some_and(|s| sources.contains(&s))
            {
                return Err("invalid UI source".into());
            }
            if let Some(v) = row.get("name") {
                let name = v.as_str().ok_or("invalid UI error name")?;
                let prefix = name.strip_suffix("Error").ok_or("invalid UI error name")?;
                if name != "Error"
                    && (prefix.is_empty()
                        || prefix.len() > 30
                        || !prefix.bytes().all(|b| b.is_ascii_alphabetic()))
                {
                    return Err("invalid UI error name".into());
                }
            }
        }
        let now = Instant::now();
        if self.arrivals.len() == 10
            && now.duration_since(self.arrivals[0]) < Duration::from_secs(1)
        {
            return Ok(429);
        }
        if self.arrivals.len() == 10 {
            self.arrivals.pop_front();
        }
        self.arrivals.push_back(now);
        for event in events {
            log.event(
                if ["error", "save_failed"].contains(&event["event"].as_str().unwrap_or("")) {
                    "WARNING"
                } else {
                    "INFO"
                },
                "UI",
                &event.to_string(),
            );
        }
        Ok(204)
    }
}
