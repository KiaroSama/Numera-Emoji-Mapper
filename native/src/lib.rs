#![forbid(unsafe_code)]

use pyo3::prelude::*;

fn greedy(hashes: &[u64]) -> Vec<usize> {
    if hashes.is_empty() {
        return Vec::new();
    }
    let mut remaining: Vec<usize> = (1..hashes.len()).collect();
    let mut ordered = Vec::with_capacity(hashes.len());
    ordered.push(0);
    let mut last = hashes[0];
    while !remaining.is_empty() {
        let (mut best, mut distance) = (0, 65);
        for (i, &index) in remaining.iter().enumerate() {
            let d = (hashes[index] ^ last).count_ones();
            if d < distance {
                best = i;
                distance = d;
                if d == 0 {
                    break;
                }
            }
        }
        // swap_remove changes first-minimum ties and therefore the pack order.
        let index = remaining.remove(best);
        ordered.push(index);
        last = hashes[index];
    }
    ordered
}

#[pyfunction]
fn greedy_indices(py: Python<'_>, hashes: Vec<u64>) -> Vec<usize> {
    py.detach(move || greedy(&hashes))
}

#[pyfunction]
fn near_indices(
    py: Python<'_>,
    hashes: Vec<Option<u64>>,
    query: u64,
    threshold: i32,
) -> PyResult<Vec<usize>> {
    if threshold != -1 && !(0..=16).contains(&threshold) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "phash threshold must be -1 or 0..16",
        ));
    }
    Ok(py.detach(move || {
        hashes
            .iter()
            .enumerate()
            .filter_map(|(i, hash)| {
                hash.filter(|h| threshold >= 0 && (h ^ query).count_ones() <= threshold as u32)
                    .map(|_| i)
            })
            .collect()
    }))
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("API_VERSION", 1)?;
    m.add_function(wrap_pyfunction!(greedy_indices, m)?)?;
    m.add_function(wrap_pyfunction!(near_indices, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::greedy;

    #[test]
    fn stable_ties_and_empty_input() {
        assert!(greedy(&[]).is_empty());
        assert_eq!(greedy(&[0, 1, 2, 3]), [0, 1, 3, 2]);
        assert_eq!(greedy(&[u64::MAX, u64::MAX, 0]), [0, 1, 2]);
    }
}
