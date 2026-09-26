use std::{collections::BTreeMap, path::Path};

use bevy::prelude::*;
use openzt2_game_data::audio::VARIANT_CUE_REFERENCE;

use crate::asset_source::AssetArchives;

use super::audio_asset_types::{AudioAsset, AudioAssets};

fn path_belongs_to_audio_document_family(path: &Path) -> bool {
    let path = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    path.starts_with("config/sound")
        || path.starts_with("entities/sound/")
        || path.starts_with("ui/sound/")
        || path.starts_with("world/sound/")
        || path.starts_with("world/soundtags/")
        || path.starts_with("world/ambients/")
        || path == "ai/ambientsmgr.xml"
}

pub(super) fn refresh_audio_assets_from_archive_overlay(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    mut index: ResMut<AudioAssets>,
) {
    let revision = archives.revision();
    if index.revision == Some(revision) {
        return;
    }
    index.handles = archives
        .resolved_paths()
        .1
        .iter()
        .cloned()
        .filter(|path| path_belongs_to_audio_document_family(path))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
        })
        .map(|path| server.load::<AudioAsset>(path))
        .collect();
    index.cues.clear();
    index.stages.clear();
    index.soundscapes.clear();
    index.allowed_entities.clear();
    index.mixer = None;
    index.ambient = None;
    index.water = None;
    index.dirty = true;
    index.revision = Some(revision);
}

pub(super) fn index_loaded_audio_assets(
    mut events: MessageReader<AssetEvent<AudioAsset>>,
    server: Res<AssetServer>,
    assets: Res<Assets<AudioAsset>>,
    mut index: ResMut<AudioAssets>,
) {
    if events
        .read()
        .any(|event| !matches!(event, AssetEvent::Unused { .. }))
    {
        index.dirty = true;
    }
    if !index.dirty
        || index.handles.iter().any(|handle| {
            !matches!(
                server.get_load_state(handle.id()),
                Some(bevy::asset::LoadState::Loaded | bevy::asset::LoadState::Failed(_))
            )
        })
    {
        return;
    }
    let mut cues = BTreeMap::new();
    let mut stages = BTreeMap::new();
    let mut soundscapes = BTreeMap::new();
    let mut allowed_entities = BTreeMap::new();
    let mut mixer = None;
    let mut ambient = None;
    let mut water = None;
    let biome_directories = index
        .handles
        .iter()
        .filter_map(|handle| assets.get(handle))
        .flat_map(|asset| &asset.document.soundscape_biomes)
        .map(|biome| {
            (
                biome.directory.trim_end_matches('/').to_ascii_lowercase(),
                biome.biome,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for handle in &index.handles {
        let Some(asset) = assets.get(handle) else {
            continue;
        };
        let directory_biome = server.get_path(handle.id()).and_then(|path| {
            path.path().parent().and_then(|directory| {
                biome_directories
                    .get(
                        &directory
                            .to_string_lossy()
                            .replace('\\', "/")
                            .to_ascii_lowercase(),
                    )
                    .copied()
            })
        });
        asset.document.cues.iter().for_each(|cue| {
            cues.insert(cue.id, handle.clone());
        });
        asset.document.stages.iter().for_each(|stage| {
            stages.insert(stage.id, handle.clone());
        });
        asset
            .document
            .soundscape_contents
            .iter()
            .enumerate()
            .for_each(|(soundscape_index, soundscape)| {
                let biome = directory_biome.unwrap_or(soundscape.biome);
                soundscapes.insert(biome, (handle.clone(), soundscape_index));
            });
        asset
            .document
            .soundscape_allowed_entities
            .iter()
            .for_each(|allowed| {
                allowed_entities.insert(allowed.biome, handle.clone());
            });
        if asset.document.mixer.is_some() {
            mixer = Some(handle.clone());
        }
        if asset.document.ambient.is_some() {
            ambient = Some(handle.clone());
        }
        if asset.document.water.is_some() {
            water = Some(handle.clone());
        }
    }
    for asset in index.handles.iter().filter_map(|handle| assets.get(handle)) {
        for cue in &asset.document.cues {
            for variant in &cue.variants {
                if variant.flags & VARIANT_CUE_REFERENCE != 0
                    && !cues.contains_key(&variant.source_id)
                {
                    warn!(
                        cue = cue.name,
                        referenced_cue = variant.source,
                        "Audio cue is unavailable; this variant cannot play"
                    );
                }
            }
        }
    }
    index.cues = cues;
    index.stages = stages;
    index.soundscapes = soundscapes;
    index.allowed_entities = allowed_entities;
    index.mixer = mixer;
    index.ambient = ambient;
    index.water = water;
    index.dirty = false;
}
