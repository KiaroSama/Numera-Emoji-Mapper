#[cfg(target_os = "linux")]
#[used]
// SAFETY: Read-only approved project logo payload placed in a dedicated ELF data section for binary verification.
#[unsafe(link_section = ".rodata.numera_logo")]
static EMBEDDED_LOGO_PNG: [u8; include_bytes!("../../assets/numera-emoji-mapper-logo-circle.png")
    .len()] = *include_bytes!("../../assets/numera-emoji-mapper-logo-circle.png");

fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match numera_runtime::worker_admin::run(&args, &mut |line| println!("{line}")) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/test_admission.rs"]
mod test_admission;
