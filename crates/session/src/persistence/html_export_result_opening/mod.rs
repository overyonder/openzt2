use std::{
    io,
    path::{Path, PathBuf},
};

use bevy::{prelude::*, tasks::IoTaskPool};

#[derive(Message)]
pub(super) struct HtmlExportWritten(pub(super) PathBuf);

pub(super) fn open_written_html_exports(mut exports: MessageReader<HtmlExportWritten>) {
    for HtmlExportWritten(path) in exports.read() {
        let path = path.clone();
        IoTaskPool::get()
            .spawn(async move {
                if let Err(error) = open_export_with_default_application(&path) {
                    warn!(path = %path.display(), %error, "could not open the exported HTML page");
                }
            })
            .detach();
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn open_export_with_default_application(path: &Path) -> io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr::null};
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

    let path: Vec<u16> = std::path::absolute(path)?
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // The path is NUL-terminated and remains allocated throughout the call.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            null(),
            path.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err(io::Error::other(format!("ShellExecuteW returned {result}")))
    }
}

#[cfg(not(windows))]
fn open_export_with_default_application(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(target_os = "macos"))]
    let program = "xdg-open";
    let status = std::process::Command::new(program)
        .arg(std::path::absolute(path)?)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{program} exited with {status}")))
    }
}
