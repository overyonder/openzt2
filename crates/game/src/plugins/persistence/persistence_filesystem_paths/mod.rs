use std::{fs, io, path::PathBuf};

use bevy::prelude::Resource;

use super::save_slot_types::SaveSlotId;

#[derive(Resource, Debug, Clone)]
pub struct PersistenceFilesystemPaths {
    persistence_root_directory: PathBuf,
}

impl PersistenceFilesystemPaths {
    pub fn new(persistence_root_directory: PathBuf) -> Self {
        Self {
            persistence_root_directory,
        }
    }

    pub fn save_slot_file_path(
        &self,
        profile_identifier: &[u8; 16],
        save_slot_identifier: SaveSlotId,
    ) -> PathBuf {
        self.profile_save_directory_path(profile_identifier)
            .join(format!("{:08x}.ozs", save_slot_identifier.0))
    }

    pub fn profile_save_directory_path(&self, profile_identifier: &[u8; 16]) -> PathBuf {
        self.persistence_root_directory
            .join("profiles")
            .join(format_binary_identifier_as_lowercase_hexadecimal(
                profile_identifier,
            ))
            .join("saves")
    }

    pub(super) fn list_existing_save_slot_file_paths(
        &self,
        profile_identifier: &[u8; 16],
    ) -> io::Result<Vec<(SaveSlotId, PathBuf)>> {
        let save_directory_path = self.profile_save_directory_path(profile_identifier);
        let directory_entries = match fs::read_dir(save_directory_path) {
            Ok(directory_entries) => directory_entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut save_slot_file_paths = directory_entries
            .filter_map(Result::ok)
            .filter_map(|directory_entry| {
                let file_path = directory_entry.path();
                let file_stem = file_path.file_stem()?.to_str()?;
                (file_path.extension()?.to_str()? == "ozs" && file_stem.len() == 8)
                    .then(|| u32::from_str_radix(file_stem, 16).ok())
                    .flatten()
                    .map(|save_slot_identifier| (SaveSlotId(save_slot_identifier), file_path))
            })
            .collect::<Vec<_>>();
        save_slot_file_paths
            .sort_unstable_by_key(|(save_slot_identifier, _)| save_slot_identifier.0);
        Ok(save_slot_file_paths)
    }

    pub fn profile_index_file_path(&self) -> PathBuf {
        self.persistence_root_directory
            .join("profiles")
            .join("index.bin")
    }

    pub fn photo_album_export_directory_path(
        &self,
        profile_identifier: &[u8; 16],
        persistent_album_identifier: u64,
    ) -> PathBuf {
        self.persistence_root_directory
            .join("profiles")
            .join(format_binary_identifier_as_lowercase_hexadecimal(
                profile_identifier,
            ))
            .join("photo-albums")
            .join(format!("{persistent_album_identifier:016x}"))
    }

    pub(super) fn overview_map_export_directory_path(
        &self,
        profile_identifier: &[u8; 16],
    ) -> PathBuf {
        self.persistence_root_directory
            .join("profiles")
            .join(format_binary_identifier_as_lowercase_hexadecimal(
                profile_identifier,
            ))
            .join("Overview Map")
    }

    pub(super) fn timestamped_full_frame_screenshot_bmp_file_path(
        &self,
        captured_unix_nanoseconds: u128,
    ) -> PathBuf {
        self.persistence_root_directory
            .join("screenshots")
            .join(format!("openzt2-{captured_unix_nanoseconds:032x}.bmp"))
    }

    pub fn challenge_photo_jpeg_file_path(
        &self,
        profile_identifier: &[u8; 16],
        challenge_identifier: &[u8; 16],
        persistent_photo_identifier: u64,
    ) -> PathBuf {
        self.persistence_root_directory
            .join("profiles")
            .join(format_binary_identifier_as_lowercase_hexadecimal(
                profile_identifier,
            ))
            .join("challenge-photos")
            .join(format_binary_identifier_as_lowercase_hexadecimal(
                challenge_identifier,
            ))
            .join(format!("{persistent_photo_identifier:016x}.jpg"))
    }
}

fn format_binary_identifier_as_lowercase_hexadecimal(identifier: &[u8; 16]) -> String {
    openzt2_game_data::AssetId(*identifier).to_lowercase_hexadecimal_string()
}
