//! Interpreter-free fixture helpers for directly ported native regression tests.
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Preserve the original streaming fixture digest, including its bounded buffer.
pub fn digest(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let read = file.read(&mut bytes).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&bytes[..read]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
