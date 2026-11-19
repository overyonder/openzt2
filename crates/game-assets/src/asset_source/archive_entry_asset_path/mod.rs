//! Bevy paths for assets tied to one immutable archive entry.

use std::path::{Path, PathBuf};

pub(crate) const ARCHIVE_ENTRY_ASSET_EXTENSION: &str = "z2entry";

pub(crate) fn archive_entry_asset_path(entry: &z2f::ResolvedArchiveEntry) -> PathBuf {
    let mut path = entry.normalized_asset_path.as_os_str().to_owned();
    path.push(format!(
        ".{}.{}.{ARCHIVE_ENTRY_ASSET_EXTENSION}",
        entry.archive_priority_index, entry.zip_entry_ordinal
    ));
    path.into()
}

pub(crate) fn archive_entry_asset_source_path(path: &Path) -> Option<(PathBuf, usize, usize)> {
    if path.extension()?.to_str()? != ARCHIVE_ENTRY_ASSET_EXTENSION {
        return None;
    }
    let stem = Path::new(path.file_stem()?);
    let ordinal = stem.extension()?.to_str()?.parse().ok()?;
    let stem = Path::new(stem.file_stem()?);
    let archive = stem.extension()?.to_str()?.parse().ok()?;
    Some((path.with_file_name(stem.file_stem()?), archive, ordinal))
}
