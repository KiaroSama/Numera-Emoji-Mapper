// Exclusive tooling fixtures retain unresolved child ownership instead of erasing recovery evidence.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Root(pub PathBuf);
impl Root {
    pub fn new(label: &str) -> Self {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let base =
            _native::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let path = base.join(format!(
            "{label} {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(_native::tooling_environment::physical_root(&path).unwrap())
    }

    pub fn clean(&self) -> Result<(), String> {
        let children = self.0.join("logs/tmp/tooling");
        _native::tooling_environment::reject_links(
            &children,
            "Tooling child storage cannot follow links; fixture retained",
        )?;
        match std::fs::read_dir(&children) {
            Ok(mut entries) => match entries.next() {
                None => {}
                Some(Ok(_)) => {
                    return Err("Tooling child ownership unresolved; fixture retained".into());
                }
                Some(Err(error)) => {
                    return Err(format!(
                        "Cannot verify tooling child cleanup; fixture retained: {error}"
                    ));
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Cannot inspect tooling child storage; fixture retained: {error}"
                ));
            }
        }
        match std::fs::remove_dir_all(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Tooling fixture cleanup failed: {error}")),
        }
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        // An assertion failure retains inputs/output even when no ownership marker
        // remains. Only an explicit successful clean may discard those diagnostics.
        if std::thread::panicking() {
            eprintln!(
                "ERROR: tooling assertion failed; fixture retained at {}",
                self.0.display()
            );
            return;
        }
        if let Err(error) = self.clean() {
            panic!("{error}");
        }
    }
}
