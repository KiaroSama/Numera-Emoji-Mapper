use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
fn main() {
    let root = PathBuf::from(std::env::var_os("NUMERA_CLI_WATCHDOG_ROOT").unwrap());
    if std::env::args().nth(1).as_deref() == Some("descendant") {
        std::fs::write(root.join("descendant.pid"), std::process::id().to_string()).unwrap();
    } else {
        let _child = Command::new(std::env::current_exe().unwrap())
            .arg("descendant")
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        std::fs::write(root.join("parent.pid"), std::process::id().to_string()).unwrap();
    }
    loop {
        std::thread::sleep(Duration::from_millis(20));
    }
}
