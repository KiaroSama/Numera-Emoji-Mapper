//! Bounded nearest-neighbour matching over native image signatures.
//!
//! Production samples are bytes. Their squared distances fit exactly in f64, so
//! the direct sum avoids cancellation when identical bright signatures are compared.
//! This is deterministic across platforms; unlike the legacy NumPy float32
//! matrix identity, it can differ where BLAS rounded or cancelled intermediates.
//! Image signatures and their serialized caches are unchanged.
pub use crate::coin_remap_policy::Nearest;

pub fn require_available() -> Result<(), String> {
    Ok(())
}

pub fn nearest(local: &[Vec<f32>], live: &[Vec<f32>]) -> Result<Nearest, String> {
    let width = live
        .first()
        .map(Vec::len)
        .filter(|width| *width > 0)
        .ok_or("remap needs nonempty live signatures")?;
    if local.iter().chain(live).any(|row| {
        row.len() != width
            || row
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=255.0).contains(value))
    }) {
        return Err("remap signatures must have equal width and finite byte-range samples".into());
    }
    if width > 4096 || live.len() > 20000 || local.len() > 100000 {
        return Err("remap numerical working set exceeds bounded signature limits".into());
    }
    let mut result = Nearest {
        indices: Vec::with_capacity(local.len()),
        squared: Vec::with_capacity(local.len()),
        second: Vec::with_capacity(local.len()),
    };
    // Keep one row's two best candidates rather than allocating a local-by-live matrix.
    for block in local.chunks(512) {
        for row in block {
            let mut best = f64::INFINITY;
            let mut second = f64::INFINITY;
            let mut index = 0;
            for (candidate, reference) in live.iter().enumerate() {
                let distance = row
                    .iter()
                    .zip(reference)
                    .map(|(left, right)| {
                        let delta = f64::from(*left) - f64::from(*right);
                        delta * delta
                    })
                    .sum::<f64>();
                if distance < best {
                    second = best;
                    best = distance;
                    index = candidate;
                } else if distance < second {
                    second = distance;
                }
            }
            result.indices.push(index);
            result.squared.push(best);
            result.second.push(second);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn native_matching_replays_every_original_numerical_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/coin-remap-math.json"
        ))
        .unwrap();
        for case in fixture["nearest"].as_array().unwrap() {
            let rows = |key: &str| {
                case[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| {
                        row.as_array()
                            .unwrap()
                            .iter()
                            .map(|value| value.as_f64().unwrap() as f32)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            };
            let output = nearest(&rows("local"), &rows("live")).unwrap();
            assert_eq!(serde_json::json!(output.indices), case["expected"]["index"]);
            assert_eq!(serde_json::json!(output.squared), case["expected"]["d2"]);
            for (actual, expected) in output
                .second
                .iter()
                .zip(case["expected"]["second"].as_array().unwrap())
            {
                assert_eq!(*actual, expected.as_f64().unwrap_or(f64::INFINITY));
            }
        }
    }

    #[test]
    fn invalid_signatures_refuse_and_duplicate_candidates_retain_first_ties() {
        assert!(nearest(&[vec![1.0]], &[]).is_err());
        assert!(nearest(&[vec![f32::NAN]], &[vec![1.0]]).is_err());
        assert!(nearest(&[vec![1.0]], &[vec![1.0, 2.0]]).is_err());
        let output = nearest(
            &[vec![255.0; 1024]],
            &[vec![255.0; 1024], vec![255.0; 1024]],
        )
        .unwrap();
        assert_eq!(output.indices, [0]);
        assert_eq!(output.squared, [0.0]);
        assert_eq!(output.second, [0.0]);
    }

    #[test]
    fn bright_signatures_and_fractional_near_ties_have_deterministic_distances() {
        let mut one_pixel = vec![255.0; 4096];
        one_pixel[4095] = 254.0;
        let output = nearest(&[vec![255.0; 4096]], &[one_pixel, vec![255.0; 4096]]).unwrap();
        assert_eq!(output.indices, [1]);
        assert_eq!(output.squared, [0.0]);
        assert_eq!(output.second, [1.0]);

        let output = nearest(
            &[vec![0.5, 254.5]],
            &[vec![0.25, 254.75], vec![0.75, 254.25], vec![0.5, 254.0]],
        )
        .unwrap();
        assert_eq!(output.indices, [0]);
        assert_eq!(output.squared, [0.125]);
        assert_eq!(output.second, [0.125]);
    }
}
