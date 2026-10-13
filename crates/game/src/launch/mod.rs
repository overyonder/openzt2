use openzt2_verification::verification_launch_arguments::{
    read_verification_launch_request_from_process_arguments, VerificationLaunchRequest,
};

/// Inputs which change how the process is assembled before Bevy starts.
pub(crate) struct GameLaunchConfiguration {
    pub(crate) enabled_z2f_archive_paths: std::io::Result<Vec<std::path::PathBuf>>,
    pub(crate) verification_request: Option<VerificationLaunchRequest>,
}

impl GameLaunchConfiguration {
    /// Reads the executable's content and optional verification arguments.
    ///
    /// # Errors
    ///
    /// Returns an error when an argument or referenced script is invalid.
    pub(crate) fn read_from_process_arguments() -> std::io::Result<Self> {
        Ok(Self {
            enabled_z2f_archive_paths: find_enabled_z2f_archives(),
            verification_request: read_verification_launch_request_from_process_arguments()?,
        })
    }
}

fn find_enabled_z2f_archives() -> std::io::Result<Vec<std::path::PathBuf>> {
    use std::{env, io};

    let directory = env::var_os("OPENZT2_Z2F_PATH")
        .map(std::path::PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| {
            env::current_exe().and_then(|executable| {
                executable
                    .parent()
                    .map(std::path::Path::to_owned)
                    .ok_or_else(|| io::Error::other("executable has no parent directory"))
            })
        })?;
    let mut archives = std::fs::read_dir(&directory)
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("Cannot read {}: {error}", directory.display()),
            )
        })?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    archives.retain(|path| {
        path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("z2f"))
    });
    archives.sort_by(|left, right| {
        let key = |path: &std::path::Path| {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let base_archive = name
                .strip_prefix('x')
                .and_then(|name| name.strip_suffix(".z2f"))
                .is_some_and(|name| {
                    name.bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'_')
                });
            (!base_archive, name)
        };
        key(left).cmp(&key(right))
    });
    let missing_archives = z2f::REQUIRED_ARCHIVES
        .iter()
        .filter(|required| {
            !archives.iter().any(|path| {
                path.file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case(required))
            })
        })
        .copied()
        .collect::<Vec<_>>();
    if !missing_archives.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Missing: {}\nSearched: {}",
                missing_archives.join(", "),
                directory.display(),
            ),
        ));
    }
    Ok(archives)
}
