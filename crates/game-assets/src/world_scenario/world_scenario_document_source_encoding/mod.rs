//! Binary encoding of canonical world source documents.

use super::{
    authored_world_scenario_state::{
        AuthoredScenarioUiModification, AuthoredScenarioUiState, AuthoredWorldScenarioState,
        ScenarioNodePolicy,
    },
    world_scenario_document_asset_and_dependency_handles::DemandLoadedAssetPath,
    world_scenario_document_source::WorldScenarioDocumentSource,
};
use openzt2_game_data::AssetId;
use openzt2_game_data::world_scenario::{WORLD_SCENARIO_VERSION, WorldScenarioDocument};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

const MAGIC: &[u8; 8] = b"OZTWORLD";
const SOURCE_VERSION: u32 = 3;
const WORLD_SCENARIO_DOCUMENT_SOURCE_HEADER_BYTE_COUNT: usize = 16;

#[derive(serde::Deserialize)]
struct LegacyScenarioSource {
    document: WorldScenarioDocument,
    starting_zoos: Box<[DemandLoadedAssetPath]>,
    terrains: Box<[DemandLoadedAssetPath]>,
    scenes: Box<[DemandLoadedAssetPath]>,
    textures: Box<[DemandLoadedAssetPath]>,
}

#[derive(serde::Deserialize)]
struct LegacyAuthoredScenarioState {
    scenario_nodes: BTreeMap<AssetId, ScenarioNodePolicy>,
    former_fence_types: BTreeMap<(AssetId, u64), AssetId>,
}

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
        || bytes[12..16] != WORLD_SCENARIO_VERSION.to_le_bytes()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid world source version",
        ));
    }
    let payload = &bytes[WORLD_SCENARIO_DOCUMENT_SOURCE_HEADER_BYTE_COUNT..];
    let decode_error = |error| io::Error::new(io::ErrorKind::InvalidData, error);
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let (source, remaining) = match version {
        SOURCE_VERSION => postcard::take_from_bytes(payload).map_err(decode_error)?,
        1 | 2 => {
            let (legacy, remaining) =
                postcard::take_from_bytes::<LegacyScenarioSource>(payload).map_err(decode_error)?;
            let mut authored = AuthoredWorldScenarioState {
                legacy_script_references: version == 1,
                ..Default::default()
            };
            let remaining = if version == 2 {
                let (policy, remaining) =
                    postcard::take_from_bytes::<LegacyAuthoredScenarioState>(remaining)
                        .map_err(decode_error)?;
                authored.scenario_nodes = policy.scenario_nodes;
                authored.former_fence_types = policy.former_fence_types;
                remaining
            } else {
                remaining
            };
            let LegacyScenarioSource {
                mut document,
                starting_zoos,
                terrains,
                scenes,
                textures,
            } = legacy;
            for scenario in &mut document.scenarios {
                if !scenario.ui_modifications.is_empty() {
                    authored.ui_modifications.insert(
                        scenario.id,
                        scenario
                            .ui_modifications
                            .drain(..)
                            .map(|rule| AuthoredScenarioUiModification {
                                document: None,
                                node: rule.node,
                                old: AuthoredScenarioUiState::default(),
                                new: AuthoredScenarioUiState {
                                    visible: Some(rule.visible),
                                    active: None,
                                },
                            })
                            .collect(),
                    );
                }
            }
            (
                WorldScenarioDocumentSource {
                    document,
                    starting_zoos,
                    terrains,
                    scenes,
                    textures,
                    authored,
                },
                remaining,
            )
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid world source version",
            ));
        }
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
