//! Actual production-crate admission controls; no fabricated crate or test-owned API substitute.
use std::path::{Path, PathBuf};

pub struct ProductionLink {
    compiler: PathBuf,
    dependencies: PathBuf,
    library: PathBuf,
}
fn files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            files(&path, out)
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|e| e == "rlib" || e == "so" || e == "dll" || e == "dylib")
        {
            out.push(path);
        }
    }
}
impl ProductionLink {
    pub fn new(root: &Path) -> Self {
        let current = std::env::current_exe().unwrap();
        let profile = current
            .ancestors()
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n == "debug" || n == "release")
            })
            .expect("test must be inside its actual Cargo profile output");
        let mut artifacts = vec![];
        files(profile, &mut artifacts);
        let test_name = current
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .rsplit_once('-')
            .unwrap()
            .0;
        let fingerprint = current
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fingerprint")
            .join(format!("test-integration-test-{test_name}.json"));
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&fingerprint).expect(
                "current test Cargo fingerprint must identify its actual production dependency",
            ))
            .unwrap();
        let expected = metadata["deps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[1] == "numera_runtime")
            .unwrap()[3]
            .as_u64()
            .unwrap()
            .to_le_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let libraries = artifacts
            .iter()
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("libnumera_runtime-")
            })
            .filter(|p| {
                let fingerprint = p
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("fingerprint/lib-numera_runtime");
                std::fs::read_to_string(fingerprint).is_ok_and(|actual| actual.trim() == expected)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            libraries.len(),
            1,
            "the actual current test dependency must identify one production rlib: {libraries:?}"
        );
        let library = libraries[0].clone();
        let dependencies = root.join("real-dependencies");
        std::fs::create_dir(&dependencies).unwrap();
        for artifact in &artifacts {
            if artifact == &library {
                continue;
            }
            let target = dependencies.join(artifact.file_name().unwrap());
            if target.exists() {
                assert_eq!(
                    std::fs::read(&target).unwrap(),
                    std::fs::read(artifact).unwrap()
                );
                continue;
            }
            if std::fs::hard_link(artifact, &target).is_err() {
                std::fs::copy(artifact, &target).unwrap();
            }
        }
        let binary = if cfg!(windows) { "rustc.exe" } else { "rustc" };
        let toolchain_homes = [
            option_env!("RUSTUP_HOME").map(PathBuf::from),
            option_env!("HOME").map(|p| Path::new(p).join(".rustup")),
            option_env!("USERPROFILE").map(|p| Path::new(p).join(".rustup")),
        ];
        let compiler = toolchain_homes
            .into_iter()
            .flatten()
            .find_map(|home| {
                std::fs::read_dir(home.join("toolchains"))
                    .ok()?
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with("1.100.0-")
                    })
                    .map(|p| p.join("bin").join(binary))
                    .find(|p| p.is_file())
            })
            .or_else(|| {
                option_env!("RUSTC")
                    .map(PathBuf::from)
                    .filter(|p| p.is_file())
            })
            .expect("the actual Rust1.100 compiler used to build this test must remain available");
        Self {
            compiler,
            dependencies,
            library,
        }
    }
    pub fn compile(
        &self,
        root: &Path,
        name: &str,
        source: &str,
        admit_production: bool,
    ) -> (std::process::Output, PathBuf) {
        let input = root.join(format!("{name}.rs"));
        std::fs::write(&input, source).unwrap();
        let output = root.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        });
        let library = if admit_production {
            self.library.clone()
        } else {
            root.join("absent-production-crate")
                .join("libnumera_runtime.rlib")
        };
        let result = std::process::Command::new(&self.compiler)
            .args(["--edition=2024", "--crate-type=bin"])
            .arg("--extern")
            .arg(format!("numera_runtime={}", library.display()))
            .arg("-L")
            .arg(format!("dependency={}", self.dependencies.display()))
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .current_dir(root)
            .output()
            .unwrap();
        (result, output)
    }
    pub fn admitted_and_executed(&self, root: &Path, name: &str, source: &str) {
        let (compiled, executable) = self.compile(root, name, source, true);
        assert!(
            compiled.status.success(),
            "real production positive control failed to link: {}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        assert!(executable.is_file());
        let executed = std::process::Command::new(&executable)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            executed.status.success(),
            "real production positive control failed: {}",
            String::from_utf8_lossy(&executed.stderr)
        );
    }
    pub fn refused(&self, root: &Path, name: &str, source: &str, admit_production: bool) -> String {
        let (compiled, executable) = self.compile(root, name, source, admit_production);
        assert!(
            !compiled.status.success(),
            "invalid production consumer unexpectedly linked"
        );
        assert!(
            !executable.exists(),
            "a refused production dependency/type cannot produce an executable"
        );
        String::from_utf8(compiled.stderr).unwrap()
    }
}
