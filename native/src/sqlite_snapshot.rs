//! Online backup includes committed WAL rows and bounds every BUSY/LOCKED retry.
use rusqlite::{
    Connection,
    backup::{Backup, StepResult},
};
use std::path::Path;
use std::time::{Duration, Instant};
pub fn copy(
    source: &Connection,
    destination: &mut Connection,
    budget: Duration,
) -> Result<(), String> {
    if budget.is_zero() {
        return Err("SQLite snapshot budget must be positive".into());
    }
    let before_source: u32 = source
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let before_destination: u32 = destination
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let result = (|| {
        source
            .busy_timeout(Duration::from_millis(25))
            .map_err(|e| e.to_string())?;
        destination
            .busy_timeout(Duration::from_millis(25))
            .map_err(|e| e.to_string())?;
        let backup = Backup::new(source, destination).map_err(|e| e.to_string())?;
        let started = Instant::now();
        loop {
            let state = backup.step(128).map_err(|e| e.to_string())?;
            if state == StepResult::Done {
                return Ok(());
            }
            if started.elapsed() >= budget {
                return Err(format!(
                    "SQLite snapshot did not finish within{}s",
                    budget.as_secs()
                ));
            }
            match state {
                StepResult::Busy | StepResult::Locked => {
                    std::thread::sleep(Duration::from_millis(25))
                }
                StepResult::More => {}
                _ => return Err("unknown SQLite snapshot state".into()),
            }
        }
    })();
    let restore_source = source
        .busy_timeout(Duration::from_millis(u64::from(before_source)))
        .map_err(|e| e.to_string());
    let restore_destination = destination
        .busy_timeout(Duration::from_millis(u64::from(before_destination)))
        .map_err(|e| e.to_string());
    result.and(restore_source).and(restore_destination)
}
pub fn backup(source: &Connection, path: &Path) -> Result<(), String> {
    backup_with_budget(source, path, Duration::from_secs(30))
}
pub fn backup_with_budget(
    source: &Connection,
    path: &Path,
    budget: Duration,
) -> Result<(), String> {
    if budget.is_zero() {
        return Err("SQLite snapshot budget must be positive".into());
    }
    let reservation = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    drop(reservation);
    let result = (|| {
        let mut destination = Connection::open(path).map_err(|e| e.to_string())?;
        copy(source, &mut destination, budget)?;
        // A recovery snapshot must be self-contained when another reader opens it.
        // Backup copies the source WAL header too; normalize only the destination.
        destination
            .execute_batch("PRAGMA journal_mode=DELETE")
            .map_err(|e| e.to_string())?;
        #[cfg(all(test, not(numera_native_runtime)))]
        if std::env::var("NUMERA_MIGRATION_TEST_ACTION").as_deref() == Ok("refuse-backup") {
            return Err("controlled backup verification failure".into());
        }
        let integrity: String = destination
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if integrity != "ok" {
            return Err(format!("backup integrity: {integrity}"));
        }
        destination.close().map_err(|(_, e)| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
    result
}
