use crate::tooling_environment;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn cardinality(count: usize) -> Result<(), String> {
    if !(1..=10000).contains(&count) {
        Err("Benchmark requires 1..10000 hashes".into())
    } else {
        Ok(())
    }
}
pub(super) fn repetitions(repeats: usize) -> Result<(), String> {
    if repeats == 0 || repeats > 21 || repeats.is_multiple_of(2) {
        Err("repeats must be odd and 1..21".into())
    } else {
        Ok(())
    }
}
pub fn generated_hashes(count: usize, seed: u64) -> Result<Vec<u64>, String> {
    cardinality(count)?;
    let mut state = seed;
    Ok((0..count)
        .map(|_| {
            state = state.wrapping_add(0x9e3779b97f4a7c15);
            let mut value = state;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            value ^ (value >> 31)
        })
        .collect())
}
fn decimal(bytes: &[u8]) -> Result<u64, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "catalog phash contains an invalid integer")?
        .trim();
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text.as_bytes()[1..]),
        Some(b'+') => (false, &text.as_bytes()[1..]),
        _ => (false, text.as_bytes()),
    };
    let mut number = 0_u64;
    let mut previous_digit = false;
    if digits.is_empty() {
        return Err("catalog phash contains an invalid integer".into());
    }
    for (index, byte) in digits.iter().copied().enumerate() {
        if byte.is_ascii_digit() {
            number = number.wrapping_mul(10).wrapping_add(u64::from(byte - b'0'));
            previous_digit = true;
        } else if byte == b'_'
            && previous_digit
            && digits.get(index + 1).is_some_and(u8::is_ascii_digit)
        {
            previous_digit = false;
        } else {
            return Err("catalog phash contains an invalid integer".into());
        }
    }
    Ok(if negative {
        number.wrapping_neg()
    } else {
        number
    })
}
fn real(value: f64) -> Result<u64, String> {
    if !value.is_finite() {
        return Err("catalog phash contains a nonfinite number".into());
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023 - 52;
    let mantissa = (bits & ((1_u64 << 52) - 1)) | (1_u64 << 52);
    let magnitude = if exponent >= 64 || exponent <= -64 {
        0
    } else if exponent >= 0 {
        mantissa.wrapping_shl(exponent as u32)
    } else {
        mantissa >> (-exponent as u32)
    };
    Ok(if value.is_sign_negative() {
        magnitude.wrapping_neg()
    } else {
        magnitude
    })
}
pub fn load_hashes(path: &Path) -> Result<Vec<u64>, String> {
    tooling_environment::reject_links(path, "benchmark catalog cannot follow links")?;
    if !path.is_file() {
        return Err("benchmark catalog must be an existing regular file".into());
    }
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())?;
    db.busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    let mut statement = db.prepare("SELECT phash FROM items WHERE phash IS NOT NULL ORDER BY position,content_key LIMIT 10001").map_err(|error| error.to_string())?;
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    let mut hashes = Vec::new();
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        let hash = match row.get_ref(0).map_err(|error| error.to_string())? {
            ValueRef::Integer(value) => value as u64,
            ValueRef::Real(value) => real(value)?,
            ValueRef::Text(bytes) | ValueRef::Blob(bytes) => decimal(bytes)?,
            ValueRef::Null => return Err("catalog returned an unexpected null phash".into()),
        };
        hashes.push(hash);
    }
    cardinality(hashes.len())?;
    Ok(hashes)
}
pub fn median(samples: &[f64]) -> Result<f64, String> {
    if samples.is_empty()
        || samples.len().is_multiple_of(2)
        || samples
            .iter()
            .any(|sample| !sample.is_finite() || *sample < 0.0)
    {
        return Err("Benchmark clock returned invalid elapsed samples".into());
    }
    let mut samples = samples.to_vec();
    samples.sort_by(f64::total_cmp);
    Ok(samples[samples.len() / 2])
}
#[derive(Debug)]
pub struct Benchmark {
    pub hashes: usize,
    pub samples: usize,
    pub median_secs: f64,
    pub order: Vec<usize>,
    pub candidates: usize,
}
impl Benchmark {
    pub fn line(&self) -> String {
        let checksum = self
            .order
            .iter()
            .fold(0xcbf29ce484222325_u64, |value, index| {
                (value ^ *index as u64).wrapping_mul(0x100000001b3)
            });
        format!(
            "Rust similarity: hashes={} samples={} median={:.6}s order_checksum={checksum:016x} shortlist_candidates={}",
            self.hashes, self.samples, self.median_secs, self.candidates
        )
    }
}
pub fn benchmark(hashes: &[u64], repeats: usize) -> Result<Benchmark, String> {
    cardinality(hashes.len())?;
    repetitions(repeats)?;
    let mut samples = Vec::with_capacity(repeats);
    let mut order = None;
    for _ in 0..repeats {
        let started = Instant::now();
        let result = crate::similarity::greedy(std::hint::black_box(hashes));
        samples.push(started.elapsed().as_secs_f64());
        if result.len() != hashes.len()
            || result.iter().copied().collect::<BTreeSet<_>>() != (0..hashes.len()).collect()
        {
            return Err("Similarity returned an invalid permutation".into());
        }
        if order.as_ref().is_some_and(|order| order != &result) {
            return Err("Similarity order changed between identical repeated inputs".into());
        }
        order = Some(result);
    }
    let values = hashes.iter().copied().map(Some).collect::<Vec<_>>();
    let mut candidates = 0;
    for query in hashes.iter().take(20) {
        for threshold in [-1, 0, 16] {
            candidates += crate::similarity::near(&values, *query, threshold)?.len();
        }
    }
    Ok(Benchmark {
        hashes: hashes.len(),
        samples: repeats,
        median_secs: median(&samples)?,
        order: order.ok_or("benchmark produced no order")?,
        candidates,
    })
}
