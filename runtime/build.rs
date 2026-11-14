// Compile the pinned renderer into release artifacts; no runtime toolchain or Python is used.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(numera_native_runtime)");
    println!("cargo:rustc-cfg=numera_native_runtime");
    println!("cargo:rerun-if-changed=vendor/rlottie");
    let target = std::env::var("TARGET").expect("Cargo target");
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
    if target.contains("apple") {
        println!("cargo:rustc-link-lib=c++");
    } else if target.contains("msvc") {
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
