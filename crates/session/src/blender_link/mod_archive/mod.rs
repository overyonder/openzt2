//! Packs the models replaced from Blender into a mod archive that the game
//! loads at its next start.

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

/// Writes `name.z2f` into `directory`, holding each replacement as `foo.glb`
/// beside the model `foo.bfb` or `foo.nif` it replaces. Rewriting a mod of
/// the same name replaces it.
pub(super) fn write_mod_archive(
    directory: &Path,
    name: &str,
    replacements: &[(PathBuf, Arc<[u8]>)],
) -> Result<PathBuf, String> {
    let valid_name = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
    if !valid_name {
        return Err("a mod name uses only letters, digits, '-' and '_'".to_owned());
    }
    // Names like x300_000 sort among the base game's archives.
    if name.starts_with(['x', 'X'])
        && name[1..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'_')
    {
        return Err(format!("{name} is reserved for the game's own archives"));
    }
    if replacements.is_empty() {
        return Err("no model has been sent from Blender yet".to_owned());
    }
    let path = directory.join(format!("{name}.z2f"));
    let partial_path = directory.join(format!("{name}.z2f.partial"));
    let write = || -> std::io::Result<()> {
        let mut archive = ZipWriter::new(std::fs::File::create(&partial_path)?);
        let options =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (model_path, gltf) in replacements {
            let entry = model_path.with_extension("glb");
            archive.start_file(entry.to_string_lossy().replace('\\', "/"), options)?;
            archive.write_all(gltf)?;
        }
        archive.finish()?.sync_all()?;
        // Renaming keeps a reader of the previous archive on its own copy.
        std::fs::rename(&partial_path, &path)
    };
    write().map_err(|error| {
        let _ = std::fs::remove_file(&partial_path);
        format!("{} could not be written: {error}", path.display())
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_holds_each_replacement_beside_the_model_it_replaces() {
        let directory = std::env::temp_dir().join(format!("openzt2-mod-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let replacements = [
            (PathBuf::from("entities/objects/hut/hut.bfb"), Arc::from(&b"glTF"[..])),
            (PathBuf::from("entities/objects/bench/bench.z2glb"), Arc::from(&b"glTF"[..])),
        ];
        let path = write_mod_archive(&directory, "my_huts", &replacements).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut names = archive.file_names().map(str::to_owned).collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            ["entities/objects/bench/bench.glb", "entities/objects/hut/hut.glb"]
        );
        assert!(archive.by_name("entities/objects/hut/hut.glb").is_ok());
        assert!(write_mod_archive(&directory, "x300_000", &replacements).is_err());
        assert!(write_mod_archive(&directory, "../escape", &replacements).is_err());
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
