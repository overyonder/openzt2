//! Binary encoding of canonical world source documents.

use super::world_scenario_document_source::WorldScenarioDocumentSource;
use openzt2_game_data::AssetId;
use openzt2_game_data::world_scenario::WORLD_SCENARIO_VERSION;
use std::{collections::BTreeSet, io};

const MAGIC: &[u8; 8] = b"OZTWORLD";
const SOURCE_VERSION: u32 = 2;
const WORLD_SCENARIO_DOCUMENT_SOURCE_HEADER_BYTE_COUNT: usize = 16;

pub fn encode_world_scenario_document_source(
    source: &WorldScenarioDocumentSource,
) -> io::Result<Vec<u8>> {
    if source.authored.legacy_script_references {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "resolve legacy world script paths before encoding",
        ));
    }
    validate_world_scenario_document_source(source)?;
    let payload = postcard::to_allocvec(source).map_err(io::Error::other)?;
    let mut bytes =
        Vec::with_capacity(WORLD_SCENARIO_DOCUMENT_SOURCE_HEADER_BYTE_COUNT + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&SOURCE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&WORLD_SCENARIO_VERSION.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

pub fn decode_world_scenario_document_source(
    bytes: &[u8],
) -> io::Result<WorldScenarioDocumentSource> {
    if bytes.len() < WORLD_SCENARIO_DOCUMENT_SOURCE_HEADER_BYTE_COUNT
        || &bytes[..8] != MAGIC
        || (bytes[8..12] != SOURCE_VERSION.to_le_bytes() && bytes[8..12] != 1_u32.to_le_bytes())
        || bytes[12..16] != WORLD_SCENARIO_VERSION.to_le_bytes()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid world source version",
        ));
    }
    let payload = &bytes[WORLD_SCENARIO_DOCUMENT_SOURCE_HEADER_BYTE_COUNT..];
    let decode_error = |error| io::Error::new(io::ErrorKind::InvalidData, error);
    let (source, remaining) = if bytes[8..12] == 1_u32.to_le_bytes() {
        let ((document, starting_zoos, terrains, scenes, textures), remaining) =
            postcard::take_from_bytes(payload).map_err(decode_error)?;
        (
            WorldScenarioDocumentSource {
                document,
                starting_zoos,
                terrains,
                scenes,
                textures,
                authored: super::authored_world_scenario_state::AuthoredWorldScenarioState {
                    legacy_script_references: true,
                    ..Default::default()
                },
            },
            remaining,
        )
    } else {
        postcard::take_from_bytes(payload).map_err(decode_error)?
    };
    if !remaining.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trailing world source bytes",
        ));
    }
    validate_world_scenario_document_source(&source)?;
    Ok(source)
}

pub fn validate_world_scenario_document_source(
    source: &WorldScenarioDocumentSource,
) -> io::Result<()> {
    let document = &source.document;
    if !unique_identifiers(document.maps.iter().map(|record| record.id))
        || !unique_identifiers(document.starting_zoos.iter().map(|record| record.id))
        || !unique_identifiers(document.locations.iter().map(|record| record.id))
        || !unique_identifiers(document.campaigns.iter().map(|record| record.id))
        || !unique_identifiers(document.scenarios.iter().map(|record| record.id))
        || !unique_identifiers(document.photo_challenges.iter().map(|record| record.id))
        || !unique_identifiers(document.photo_challenge_sets.iter().map(|record| record.id))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "duplicate world record identity",
        ));
    }
    for paths in [
        &source.starting_zoos,
        &source.terrains,
        &source.scenes,
        &source.textures,
    ] {
        if !unique_identifiers(paths.iter().map(|path| path.id)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "duplicate world dependency identity",
            ));
        }
        for dependency in paths.iter() {
            let path = bevy::asset::AssetPath::try_parse(&dependency.path)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            if path.path().as_os_str().is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "world dependency path is empty",
                ));
            }
        }
    }
    Ok(())
}

fn unique_identifiers(identifiers: impl Iterator<Item = AssetId>) -> bool {
    let mut seen = BTreeSet::new();
    identifiers
        .into_iter()
        .all(|identifier| seen.insert(identifier))
}
