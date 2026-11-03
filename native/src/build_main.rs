//! Contributor-only entry point: never occupies the application executable being rebuilt.
#![deny(unsafe_code)]

fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let code = match _native::tooling_environment::project_root() {
        Ok(root)
            if args
                .first()
                .is_some_and(|argument| argument == "runtime-probe") =>
        {
            // SAFETY: first dispatch in a fresh binary, before threads, Tokio or Python attachment.
            #[allow(unsafe_code)]
            unsafe {
                _native::tooling_runtime::probe(&root, &args[1..])
            }
        }
        Ok(root) => _native::tooling_build::run(&root, &args),
        Err(error) => {
            eprintln!("ERROR: {}", _native::logging::redact(&error, &[]));
            1
        }
    };
    // All process owners and logs have dropped; Windows preserves full child status.
    #[cfg(windows)]
    std::process::exit(code);
    #[cfg(not(windows))]
    std::process::ExitCode::from(code as u8)
}
