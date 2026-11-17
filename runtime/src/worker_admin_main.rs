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
