//! Public binary delegates only to the standalone application, never the Python oracle library.
fn main() -> std::process::ExitCode {
    numera_runtime::entry()
}
