use std::path::{Path, PathBuf};

/// Returns the case-insensitive slash form used by the archive overlay.
#[must_use]
pub fn normalize_asset_path_for_case_insensitive_archive_lookup(path: &Path) -> PathBuf {
    PathBuf::from(
        path.to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase(),
    )
}
