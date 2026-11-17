//! Canonical catalog lease nesting is same-thread/same-mode; OS ownership excludes other writers.
use crate::locks::Lease;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Writer,
    Maintenance,
}
struct Held {
    mode: Mode,
    _lease: Lease,
}
thread_local! {
    static OWNERS: RefCell<BTreeMap<PathBuf,Weak<Held>>> = const { RefCell::new(BTreeMap::new()) };
}
pub struct Ownership {
    _held: Arc<Held>,
}
impl Ownership {
    pub fn acquire(directory: &Path, mode: Mode, utc: &str) -> Result<Self, String> {
        let canonical = directory.canonicalize().map_err(|e| e.to_string())?;
        #[cfg(windows)]
        let key = PathBuf::from(canonical.to_string_lossy().to_lowercase());
        #[cfg(not(windows))]
        let key = canonical.clone();
        let held = OWNERS.with(|owners| -> Result<Arc<Held>,String> {
            let mut owners = owners.borrow_mut();
            let held = if let Some(held) = owners.get(&key).and_then(Weak::upgrade) {
                if held.mode != mode { return Err("catalog already has incompatible ownership in this thread".into()); }
                held
            } else {
                let lease=Lease::acquire(&canonical.join(".maintenance.lock"),utc).map_err(|e|format!("catalog ownership refused: {e}"))?;
                let held=Arc::new(Held { mode, _lease:lease });
                owners.insert(key,Arc::downgrade(&held));
                held
            };
            if mode==Mode::Writer && canonical.join("identity-migration.journal.json").exists() {
                return Err("catalog has an interrupted identity migration; resume or restore before writing".into());
            }
            Ok(held)
        })?;
        Ok(Self { _held: held })
    }
}
