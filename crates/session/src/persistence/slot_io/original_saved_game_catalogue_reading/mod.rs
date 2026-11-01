use std::{io, path::Path, time::UNIX_EPOCH};

use openzt2_game_assets::{
    original_saved_game_source::OriginalSavedGameSources,
    world_scenario::saved_world_catalogue_metadata::read_saved_world_display_name,
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::persistence::saved_game_source_types::OriginalSavedGameRecord;

use super::super::{
    original_profile_save_locations::OriginalProfileSaveLocations,
    snapshot_container_encoding_and_validation::MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT,
};

pub(super) struct OriginalSavedGameCatalogueRead {
    pub records: Vec<OriginalSavedGameRecord>,
    pub failures: Vec<String>,
}

pub(super) fn read_original_saved_game_catalogue(
    locations: &OriginalProfileSaveLocations,
    sources: &OriginalSavedGameSources,
) -> io::Result<OriginalSavedGameCatalogueRead> {
    let files = locations.discover()?;
    let mut records = Vec::with_capacity(files.len());
    let mut failures = Vec::new();
    for file in files {
        let read_record = || -> io::Result<OriginalSavedGameRecord> {
            let source_identifier = sources.register(&file.save_file_path)?;
            let bytes = sources.read_member(
                source_identifier,
                Path::new("saved/world.zt2"),
                MAXIMUM_WORLD_SNAPSHOT_CONTAINER_BYTE_COUNT,
            )?;
            let world_display_name = read_saved_world_display_name(&bytes)?;
            let modified = file.save_file_path.metadata()?.modified()?;
            let timestamp = modified.duration_since(UNIX_EPOCH).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "saved world timestamp precedes the Unix epoch",
                )
            })?;
            let profile = file.profile_directory.canonicalize()?;
            let mut hash = blake3::Hasher::new();
            hash.update(b"original-saved-profile\0");
            hash.update(profile.as_os_str().as_encoded_bytes());
            let mut identifier = [0; 16];
            identifier.copy_from_slice(&hash.finalize().as_bytes()[..16]);
            let profile_display_name = profile
                .file_name()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "saved profile has no directory name",
                    )
                })?
                .to_string_lossy()
                .into_owned();
            Ok(OriginalSavedGameRecord {
                source_identifier,
                world_display_name,
                profile_identifier: AssetId(identifier),
                profile_display_name,
                last_saved_unix_timestamp_milliseconds: u64::try_from(timestamp.as_millis())
                    .map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "saved world timestamp exceeds u64 milliseconds",
                        )
                    })?,
            })
        };
        match read_record() {
            Ok(record) => records.push(record),
            Err(error) => failures.push(format!("{}: {error}", file.save_file_path.display())),
        }
    }
    records.sort_by_key(|record| {
        (
            std::cmp::Reverse(record.last_saved_unix_timestamp_milliseconds),
            record.source_identifier,
        )
    });
    Ok(OriginalSavedGameCatalogueRead { records, failures })
}
