//! Content-match admission is separate from signature math and durable map replacement.
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

pub struct Match {
    pub ticker: String,
    pub cid: String,
    pub distance: f64,
    pub margin: f64,
}
#[derive(Clone, Copy)]
pub struct Options {
    pub apply: bool,
    pub max_distance: Option<f64>,
    pub min_margin: Option<f64>,
    pub incomplete_sets: usize,
    pub failed_images: usize,
}
pub struct Decision {
    pub mapping: Map<String, Value>,
    pub far: Vec<String>,
    pub ambiguous: Vec<String>,
    pub code: u8,
    pub destination: Option<&'static str>,
    pub backup: bool,
}

pub fn decide(matches: &[Match], options: Options) -> Result<Decision, String> {
    if options
        .max_distance
        .is_some_and(|n| !n.is_finite() || n < 0.0)
        || options
            .min_margin
            .is_some_and(|n| !n.is_finite() || n < 0.0)
        || options.apply && options.max_distance.is_none_or(|n| n <= 0.0)
    {
        return Err(
            "remap apply needs a finite calibrated --max-distance > 0 and nonnegative margin"
                .into(),
        );
    }
    let mut mapping = Map::new();
    let mut far = Vec::new();
    let mut ambiguous = Vec::new();
    let mut seen = BTreeSet::new();
    for item in matches {
        if item.ticker.is_empty()
            || item.cid.is_empty()
            || !seen.insert(&item.ticker)
            || !item.distance.is_finite()
            || item.distance < 0.0
            || item.margin.is_nan()
            || item.margin < 0.0
        {
            return Err("invalid or duplicate remap match evidence".into());
        }
        if let Some(max) = options.max_distance {
            if item.distance > max {
                far.push(item.ticker.clone());
                continue;
            }
            if item.margin < options.min_margin.unwrap_or(max) {
                ambiguous.push(item.ticker.clone());
                continue;
            }
        }
        mapping.insert(item.ticker.clone(), json!(item.cid));
    }
    let blocked = options.incomplete_sets > 0 || options.failed_images > 0 || !ambiguous.is_empty();
    Ok(Decision {
        mapping,
        far,
        ambiguous,
        code: if options.apply && blocked { 3 } else { 0 },
        destination: if !options.apply {
            None
        } else if blocked {
            Some("candidate")
        } else {
            Some("map")
        },
        backup: options.apply && !blocked,
    })
}
