#[cfg(target_os = "linux")]
#[used]
// SAFETY: Read-only approved project logo payload placed in a dedicated ELF section kept across LTO and --gc-sections.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".numera_logo")]
static EMBEDDED_LOGO_PNG: [u8; include_bytes!("../../assets/numera-emoji-mapper-logo-circle.png")
    .len()] = *include_bytes!("../../assets/numera-emoji-mapper-logo-circle.png");

fn main() -> std::process::ExitCode {
    #[cfg(target_os = "linux")]
    std::hint::black_box(&EMBEDDED_LOGO_PNG);
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match numera_runtime::worker_admin::run(&args, &mut |line| println!("{line}")) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("{}", numera_runtime::logging::redact(&error, &[]));
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/test_admission.rs"]
mod test_admission;
