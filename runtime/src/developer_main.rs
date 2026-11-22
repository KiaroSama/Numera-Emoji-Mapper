#[cfg(target_os = "linux")]
#[used]
// SAFETY: Read-only approved project logo payload placed in a dedicated ELF data section for binary verification.
#[unsafe(link_section = ".rodata.numera_logo")]
static EMBEDDED_LOGO_PNG: [u8; include_bytes!("../../assets/numera-emoji-mapper-logo-circle.png")
    .len()] = *include_bytes!("../../assets/numera-emoji-mapper-logo-circle.png");

fn main() -> std::process::ExitCode {
    numera_runtime::developer::main()
}

#[cfg(test)]
#[path = "../tests/support/test_admission.rs"]
mod test_admission;
