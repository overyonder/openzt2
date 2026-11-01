//! Discovery of original saved zoos without changing their profile or files.

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs, io,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone)]
pub(super) struct OriginalSavedGameFile {
    pub profile_directory: PathBuf,
    pub save_file_path: PathBuf,
}

/// Searches immediate profiles beneath the configured product directories.
/// The caller obtains the relative product path from the profile options.
pub(super) fn discover_original_saved_game_files(
    roaming_directories: impl IntoIterator<Item = PathBuf>,
    wine_prefixes: impl IntoIterator<Item = PathBuf>,
    relative_product_directory: &Path,
) -> io::Result<Vec<OriginalSavedGameFile>> {
    if relative_product_directory.as_os_str().is_empty()
        || relative_product_directory
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "profile product directory must be relative",
        ));
    }
    let mut roaming_directories = roaming_directories.into_iter().collect::<Vec<_>>();
    for prefix in wine_prefixes {
        let Some(users) = existing_child(&prefix.join("drive_c"), "users")? else {
            continue;
        };
        for user in immediate_directories(&users)? {
            let Some(app_data) = existing_child(&user, "AppData")? else {
                continue;
            };
            if let Some(roaming) = existing_child(&app_data, "Roaming")? {
                roaming_directories.push(roaming);
            }
        }
    }
    let mut seen_files = BTreeSet::new();
    let mut saves = Vec::new();
    for roaming in roaming_directories {
        let mut product = Some(roaming);
        for component in relative_product_directory.components() {
            let Component::Normal(component) = component else {
                unreachable!("validated relative product directory");
            };
            product = match product {
                Some(parent) => existing_child_os(&parent, component)?,
                None => break,
            };
        }
        let Some(product) = product else { continue };
        for profile in immediate_directories(&product)? {
            let Some(saved) = existing_child(&profile, "Saved")? else {
                continue;
            };
            for entry in fs::read_dir(saved)? {
                let entry = entry?;
                if !fs::metadata(entry.path())?.is_file()
                    || !entry.path().extension().is_some_and(|extension| {
                        extension.as_encoded_bytes().eq_ignore_ascii_case(b"z2s")
                    })
                {
                    continue;
                }
                let save_file_path = entry.path().canonicalize()?;
                if seen_files.insert(save_file_path.clone()) {
                    saves.push(OriginalSavedGameFile {
                        profile_directory: profile.canonicalize()?,
                        save_file_path,
                    });
                }
            }
        }
    }
    saves.sort_unstable_by(|left, right| left.save_file_path.cmp(&right.save_file_path));
    Ok(saves)
}

fn immediate_directories(parent: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry?;
        if fs::metadata(entry.path())?.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort_unstable();
    Ok(directories)
}

fn existing_child(parent: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    existing_child_os(parent, OsStr::new(name))
}

fn existing_child_os(parent: &Path, name: &OsStr) -> io::Result<Option<PathBuf>> {
    let exact = parent.join(name);
    match exact.try_exists() {
        Ok(true) => return Ok(Some(exact)),
        Ok(false) => {}
        Err(error) => return Err(error),
    }
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut matches = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry
            .file_name()
            .as_encoded_bytes()
            .eq_ignore_ascii_case(name.as_encoded_bytes())
            && fs::metadata(entry.path())?.is_dir()
        {
            matches.push(entry.path());
        }
    }
    if matches.len() > 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "ambiguous profile directory {}",
                parent.join(name).display()
            ),
        ));
    }
    Ok(matches.pop())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_finds_immediate_profiles_and_preserves_original_files() -> io::Result<()> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "openzt2-original-profile-{}-{unique}",
            std::process::id()
        ));
        let wine = root.join("wine");
        let roaming = wine.join("drive_c/users/Test User/appdata/roaming");
        let saved = roaming.join("Microsoft Games/Zoo Tycoon 2/Original Profile/sAvEd");
        fs::create_dir_all(&saved)?;
        fs::write(saved.join("First.Z2S"), b"unchanged original")?;
        fs::write(saved.join("native.ozs"), b"native")?;
        fs::create_dir_all(saved.join("nested"))?;
        fs::write(saved.join("nested/not-a-profile.z2s"), b"nested")?;
        let other = roaming.join("Microsoft Games/Zoo Tycoon 2/Other Profile/Saved");
        fs::create_dir_all(&other)?;
        fs::write(other.join("Second.z2s"), b"another original")?;
        let files = discover_original_saved_game_files(
            [roaming.clone(), roaming.clone()],
            [wine, root.join("missing-prefix")],
            Path::new("Microsoft Games/Zoo Tycoon 2"),
        )?;
        assert_eq!(files.len(), 2);
        let product = saved.parent().unwrap().parent().unwrap().canonicalize()?;
        assert!(files
            .iter()
            .all(|file| file.profile_directory.parent() == Some(product.as_path())));
        assert_eq!(fs::read(saved.join("First.Z2S"))?, b"unchanged original");
        assert!(discover_original_saved_game_files([], [], Path::new("../outside")).is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
