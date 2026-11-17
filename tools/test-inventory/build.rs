use std::{path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=RUSTC");
    let compiler = std::env::var_os("RUSTC").expect("Cargo selects the actual compiler");
    let output = Command::new(compiler)
        .args(["--print", "sysroot"])
        .output()
        .expect("inspect actual compiler sysroot");
    assert!(
        output.status.success(),
        "actual compiler sysroot inspection failed"
    );
    let sysroot = String::from_utf8(output.stdout).expect("UTF-8 compiler sysroot");
    let binary = Path::new(sysroot.trim()).join("bin");
    for (key, name) in [
        ("NUMERA_INVENTORY_RUSTC", "rustc"),
        ("NUMERA_INVENTORY_RUSTDOC", "rustdoc"),
    ] {
        let path = binary.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        assert!(path.is_file(), "actual compiler component is missing");
        println!("cargo:rustc-env={key}={}", path.display());
    }
}
