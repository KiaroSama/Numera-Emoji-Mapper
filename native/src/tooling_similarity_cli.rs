//! Contributor-only benchmark orchestration, without dotenv or production application imports.
use crate::{logging::RunLog, tooling_similarity_benchmark as benchmark};
use std::path::Path;

pub fn run(root: &Path, args: &[String]) -> u8 {
    let options = match benchmark::arguments(args) {
        Ok(benchmark::Arguments::Exit(code)) => return code,
        Ok(benchmark::Arguments::Catalog(path)) => path,
        Err(error) => {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
            return 1;
        }
    };
    let log = std::sync::Arc::new(RunLog::open(root, "benchmark_similarity", vec![]));
    log.install_panic_hook();
    let result = (|| {
        let catalog = options
            .map(|path| {
                if path.is_absolute() {
                    Ok(path)
                } else {
                    std::env::current_dir()
                        .map(|cwd| cwd.join(path))
                        .map_err(|e| e.to_string())
                }
            })
            .transpose()
            .map_err(benchmark::Failure::Verification)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| {
                benchmark::Failure::Uncaught("Cannot initialize benchmark process monitor".into())
            })?;
        let executable = std::env::current_exe()
            .map_err(|error| benchmark::Failure::Uncaught(error.to_string()))?;
        runtime.block_on(async {
            let source = crate::source_git_inventory::read_source(
                root,
                "1a36a968b006236bd0665a1da65c541938c06061",
                "tests/_similarity_oracle.py",
            )
            .await
            .map_err(benchmark::Failure::Uncaught)?;
            crate::source_oracle_worker::benchmark_similarity(
                root,
                &executable,
                &source,
                catalog.as_deref(),
            )
            .await
        })
    })();
    if let Ok(report) = &result {
        println!("{}", report.line());
    }
    let code = benchmark::finish(result, &log);
    log.finish(i32::from(code));
    code
}
