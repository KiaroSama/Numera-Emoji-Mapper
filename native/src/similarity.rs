//! Exact Hamming policies shared by native consumers and external Python bindings.
pub fn greedy(hashes: &[u64]) -> Vec<usize> {
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

pub fn near(hashes: &[Option<u64>], query: u64, threshold: i32) -> Result<Vec<usize>, String> {
    if threshold != -1 && !(0..=16).contains(&threshold) {
        return Err("phash threshold must be -1 or 0..16".into());
    }
    Ok(hashes
        .iter()
        .enumerate()
        .filter_map(|(i, hash)| {
            hash.filter(|h| threshold >= 0 && (h ^ query).count_ones() <= threshold as u32)
                .map(|_| i)
        })
        .collect())
}
