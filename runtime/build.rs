// Compile the pinned renderer into release artifacts; no runtime toolchain or Python is used.
fn msvc_compiler_metadata(target: &str) {
    if !target.ends_with("-windows-msvc") {
        return;
    }
    assert_eq!(target, "x86_64-pc-windows-msvc", "supported MSVC target");
    struct DiscoveryEnvironment;
    impl find_msvc_tools::EnvGetter for DiscoveryEnvironment {
        fn get_env(&self, name: &'static str) -> Option<find_msvc_tools::Env> {
            // Force installation discovery instead of a developer-prompt PATH
            // shortcut; never append ambient search paths to SDK metadata.
            if matches!(
                name,
                "VCINSTALLDIR" | "VSTEL_MSBuildProjectFullPath" | "PATH" | "LIB" | "INCLUDE"
            ) {
                return None;
            }
            println!("cargo:rerun-if-env-changed={name}");
            std::env::var_os(name).map(find_msvc_tools::Env::Owned)
        }
    }
    let tool = find_msvc_tools::find_tool_with_env("x86_64", "link.exe", &DiscoveryEnvironment)
        .expect("installed MSVC linker and Windows SDK are required");
    let linker = tool.path();
    assert!(
        linker.is_absolute()
            && linker.is_file()
            && linker
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("link.exe")),
        "discovered MSVC linker must be an absolute existing link.exe"
    );
    let libraries = tool
        .env()
        .into_iter()
        .find_map(|(name, value)| (name == "LIB").then_some(value))
        .expect("discovered MSVC tool must provide library search directories");
    // The crate appends an empty inherited tail when LIB is absent. Do not
    // admit that implicit current-directory search into the isolated runner.
    let directories = std::env::split_paths(libraries)
        .filter(|path| !path.as_os_str().is_empty())
        .collect::<Vec<_>>();
    assert!(
        !directories.is_empty()
            && directories
                .iter()
                .all(|path| path.is_absolute() && path.is_dir()),
        "discovered MSVC library directories must be absolute and available"
    );
    let libraries = std::env::join_paths(directories).expect("MSVC library search path");
    for (name, value) in [
        ("NUMERA_BASELINE_MSVC_LINKER", linker.as_os_str()),
        ("NUMERA_BASELINE_MSVC_LIB", libraries.as_os_str()),
    ] {
        let value = value.to_str().expect("UTF-8 MSVC build metadata");
        assert!(
            !value.contains(['\r', '\n']),
            "MSVC build metadata must remain a single Cargo directive"
        );
        println!("cargo:rustc-env={name}={value}");
    }
}

fn main() {
    // Test fixtures need the actual compiler after the runner removes Rustup's
    // user configuration. Resolve Cargo's compiler before entering isolation.
    println!("cargo:rerun-if-env-changed=RUSTC");
    let compiler = std::env::var_os("RUSTC").expect("Cargo selects the actual compiler");
    let output = std::process::Command::new(compiler)
        .args(["--print", "sysroot"])
        .output()
        .expect("inspect actual compiler sysroot");
    assert!(
        output.status.success(),
        "compiler sysroot inspection failed"
    );
    let sysroot = String::from_utf8(output.stdout).expect("UTF-8 compiler sysroot");
    let compiler = std::path::Path::new(sysroot.trim())
        .join("bin")
        .join(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    assert!(compiler.is_file(), "actual compiler component is missing");
    println!(
        "cargo:rustc-env=NUMERA_BASELINE_RUSTC={}",
        compiler.display()
    );

    println!("cargo:rerun-if-changed=vendor/rlottie");
    let target = std::env::var("TARGET").expect("Cargo target");
    msvc_compiler_metadata(&target);
    let mut config = cmake::Config::new("vendor/rlottie");
    config
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("LOTTIE_MODULE", "OFF") // Include the image decoder instead of a runtime plugin.
        .define("LOTTIE_THREAD", "ON")
        .define("LOTTIE_CACHE", "ON")
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
        .define("CMAKE_EXPORT_NO_PACKAGE_REGISTRY", "ON")
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        .build_target("rlottie");
    if target.contains("msvc") {
        // Match Rust and the other native dependencies instead of mixing /MT and /MD.
        let features = std::env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
        config.static_crt(features.split(',').any(|feature| feature == "crt-static"));
    }
    let destination = config.build();
    println!(
        "cargo:rustc-link-search=native={}/build",
        destination.display()
    );
    // Visual Studio generators place the archive in a configuration subdirectory.
    for profile in ["Release", "Debug", "RelWithDebInfo", "MinSizeRel"] {
        println!(
            "cargo:rustc-link-search=native={}/build/{profile}",
            destination.display()
        );
    }
    println!("cargo:rustc-link-lib=static=rlottie");
    if target.contains("msvc") {
        println!("cargo:rustc-link-lib=Shlwapi");
    } else {
        println!("cargo:rustc-link-lib=stdc++");
        if target.contains("windows") {
            println!("cargo:rustc-link-lib=shlwapi");
        } else {
            println!("cargo:rustc-link-lib=pthread");
        }
    }
}
