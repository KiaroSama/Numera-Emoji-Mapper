//! Npm fixtures retain unresolved native children and failed-assertion evidence.
use crate::tooling_environment;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Root(pub PathBuf);

impl Root {
    pub(super) fn new(label: &str) -> Self {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let base = tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let path = base.join(format!(
            "developer-npm {label} {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(tooling_environment::physical_root(&path).unwrap())
    }

    pub(super) fn clean(&self) -> Result<(), String> {
        let children = self.0.join("logs/tmp/tooling");
        tooling_environment::reject_links(
            &children,
            "Npm fixture child storage cannot follow links; evidence retained",
        )?;
        match std::fs::read_dir(&children) {
            Ok(mut entries) => match entries.next() {
                None => {}
                Some(Ok(_)) => return Err("Npm fixture child ownership is unresolved".into()),
                Some(Err(error)) => {
                    return Err(format!("Cannot verify npm fixture child cleanup: {error}"));
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Cannot inspect npm fixture child storage: {error}")),
        }
        match std::fs::remove_dir_all(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Cannot clean npm fixture: {error}")),
        }
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "{} [ERROR] [npm-fixture] Assertion failed; evidence retained at {}",
                crate::logging::iso_utc(),
                self.0.display()
            );
            return;
        }
        if let Err(error) = self.clean() {
            panic!("{error}; evidence retained at {}", self.0.display());
        }
    }
}
