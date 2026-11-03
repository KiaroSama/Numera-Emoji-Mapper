//! Installed native menu entry point; the retained PowerShell bootstrap is not switched here.
use crate::{
    config::Config,
    logging::RunLog,
    tooling_environment,
    tooling_interrupt::Interrupt,
    tooling_launch,
    tooling_menu::{self, Request},
};
use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::Arc;

pub fn run(root: &Path, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-emoji menu\nInteractive installed-runtime launcher. Never builds or installs prerequisites. Ctrl+C stops an active command; exit quits the menu."
        );
        return 0;
    }
    let config = match Config::load(root.into()) {
        Ok(config) => config,
        Err(_) => {
            eprintln!("ERROR: cannot read launcher configuration");
            return 1;
        }
    };
    let log = Arc::new(RunLog::open(root, "menu", config.secret_values()));
    log.install_panic_hook();
    let result = (|| -> Result<(), String> {
        if !args.is_empty() {
            return Err("menu accepts no arguments".into());
        }
        let root = tooling_environment::physical_root(root)?;
        let inherited = std::env::vars().collect::<BTreeMap<_, _>>();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "cannot initialize launcher child owner")?;
        let stdin = std::io::stdin();
        let mut input: Box<dyn std::io::BufRead> = if stdin.is_terminal() {
            Box::new(stdin.lock())
        } else {
            #[cfg(windows)]
            let handle = {
                use std::os::windows::io::AsHandle;
                stdin.as_handle().try_clone_to_owned()
            };
            #[cfg(unix)]
            let handle = {
                use std::os::fd::AsFd;
                stdin.as_fd().try_clone_to_owned()
            };
            #[cfg(not(any(unix, windows)))]
            return Err("launcher redirected input is unsupported on this platform".into());
            #[cfg(any(unix, windows))]
            {
                let file = std::fs::File::from(handle.map_err(|_| "cannot retain launcher input")?);
                // The child shares the OS stream offset. Reading ahead would steal its
                // confirmation, or consume the next menu action before the child exits.
                Box::new(std::io::BufReader::with_capacity(1, file))
            }
        };
        let mut output = std::io::stdout();
        let color = output.is_terminal() && !inherited.contains_key("NO_COLOR");
        tooling_menu::run(&root, &mut input, &mut output, color, |request| {
            std::io::stdout()
                .flush()
                .map_err(|_| "cannot flush launcher output")?;
            let check = vec!["check".into()];
            let args = match request {
                Request::Checks => &check,
                Request::Command(args) => args,
            };
            let launch = tooling_launch::plan(&root, args, &inherited)?;
            log.event(
                "INFO",
                "launcher",
                &format!(
                    "action={} argc={}",
                    args.first().map(String::as_str).unwrap_or("?"),
                    args.len()
                ),
            );
            let mut interrupt = Interrupt::install()?;
            let result =
                runtime.block_on(tooling_launch::foreground(&launch, interrupt.wait(), &log));
            // Restore prompt behavior only after foreground ownership has ended.
            let interrupted = interrupt.interrupted();
            interrupt.close()?;
            result.map(|code| if interrupted { 130 } else { code })
        })
    })();
    let code = match result {
        Ok(()) => 0,
        Err(error) => {
            log.event("ERROR", "launcher", &error);
            1
        }
    };
    log.finish(code)
}
