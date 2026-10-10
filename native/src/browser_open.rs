//! Open only the panel's numeric-loopback page, never arbitrary caller-supplied URLs.
#![allow(unsafe_code)]

pub fn panel_url(port: u16, no_open: bool) -> Option<String> {
    (!no_open && port != 0).then(|| format!("http://127.0.0.1:{port}/"))
}

pub async fn open(port: u16, no_open: bool) -> Result<(), String> {
    let Some(url) = panel_url(port, no_open) else {
        return Ok(());
    };
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || {
            use windows_sys::Win32::System::Com::{
                COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
            };
            use windows_sys::Win32::UI::Shell::ShellExecuteW;
            let initialized = unsafe {
                CoInitializeEx(
                    std::ptr::null(),
                    (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
                )
            };
            let verb = [111u16, 112, 101, 110, 0];
            let wide = url.encode_utf16().chain([0]).collect::<Vec<_>>();
            let result = unsafe {
                ShellExecuteW(
                    std::ptr::null_mut(),
                    verb.as_ptr(),
                    wide.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    1,
                )
            } as isize;
            if initialized >= 0 {
                unsafe {
                    CoUninitialize();
                }
            }
            if result > 32 {
                Ok(())
            } else {
                Err(format!(
                    "default browser did not open (shell code {result})"
                ))
            }
        })
        .await
        .map_err(|_| "browser opener failed")?
    }
    #[cfg(not(windows))]
    {
        let mut command = tokio::process::Command::new(if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        });
        command
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|_| "default browser opener is unavailable")?;
        match tokio::time::timeout(std::time::Duration::from_secs(10), child.wait()).await {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(_) => Err("default browser did not open".into()),
            Err(_) => {
                let _ = child.kill().await;
                Err("default browser opener exceeded10s".into())
            }
        }
    }
}
