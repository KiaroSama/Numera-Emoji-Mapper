fn main() -> std::process::ExitCode {
    let proxy = std::env::args_os()
        .next()
        .and_then(|path| std::path::Path::new(&path).file_name().map(|s| s == "git"))
        .unwrap_or(false);
    let result = if proxy {
        numera_ci::git_proxy()
    } else {
        numera_ci::scan()
    };
    match result {
        Ok(code) => std::process::ExitCode::from(code),
        Err(message) => {
            // Errors are static, reviewed messages. Never print raw scanner output,
            // subprocess errors, JSON findings, source snippets, or commit messages.
            eprintln!("[ERROR] {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "../../../../runtime/tests/support/test_admission.rs"]
mod test_admission;
