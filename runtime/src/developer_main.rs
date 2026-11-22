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
    numera_runtime::developer::main()
}

#[cfg(test)]
#[path = "../tests/support/test_admission.rs"]
mod test_admission;
