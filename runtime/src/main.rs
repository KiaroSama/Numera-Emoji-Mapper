fn main() -> std::process::ExitCode {
    numera_runtime::entry()
}

#[cfg(test)]
#[path = "../tests/support/test_admission.rs"]
mod test_admission;
