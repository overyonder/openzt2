//! Archive-revision discovery and precedence indexing for world-scenario assets.

use std::{collections::BTreeMap, path::Path};

use bevy::prelude::*;

use crate::asset_source::AssetArchives;

use super::{
    world_scenario_asset_set_state_and_borrowing_queries::{
        ScenarioDocumentHandle, WorldScenarios,
    },
    world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};

fn scenario_catalogue_path(path: &Path) -> bool {
    let path = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    path.starts_with("locations/")
        || path.starts_with("photochall/")
        || path.starts_with("scenario/campaign/")
        || matches!(path.as_str(), "maps/index.xml" | "maps/scenario/index.xml")
}

pub(super) fn refresh_world_scenario_asset_set_after_archive_revision(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    mut scenarios: ResMut<WorldScenarios>,
) {
    let revision = archives.revision();
    if scenarios.revision == Some(revision) {
        return;
    }
    scenarios.handles = archives
        .resolved_paths()
        .1
        .iter()
        .cloned()
        .filter(|path| scenario_catalogue_path(path))
        .map(|path| server.load(path))
        .collect();
    scenarios.revision = Some(revision);
    scenarios.dirty = true;
}

pub(super) fn index_loaded_world_scenario_assets_by_identifier(
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut scenarios: ResMut<WorldScenarios>,
    mut events: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
) {
    let changed = events.read().any(|event| match event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::Removed { id }
        | AssetEvent::LoadedWithDependencies { id }
        | AssetEvent::Unused { id } => {
            scenarios.handles.iter().any(|handle| handle.id() == *id)
                || scenarios.handles.iter().any(|handle| {
                    assets.get(handle).is_some_and(|asset| {
                        asset
                            .campaign_scenarios
                            .iter()
                            .any(|scenario| scenario.handle.id() == *id)
                    })
                })
        }
    });
    if !scenarios.dirty && !changed {
        return;
    }
    let mut maps = BTreeMap::new();
    let mut locations = BTreeMap::new();
    let mut starts = BTreeMap::new();
    let mut campaigns = BTreeMap::new();
    let mut campaign_order = Vec::new();
    let mut scenario_records = BTreeMap::new();
    let mut photo_challenges = BTreeMap::new();
    let mut photo_sets = BTreeMap::new();
    let mut terrains = BTreeMap::new();
    let mut textures = BTreeMap::new();
    let mut start_paths = BTreeMap::new();
    for handle in &scenarios.handles {
        let Some(asset) = assets.get(handle) else {
            continue;
        };
        asset.document.locations.iter().for_each(|record| {
            locations.insert(record.id, handle.clone());
        });
        asset.document.maps.iter().for_each(|record| {
            maps.insert(record.id, handle.clone());
        });
        asset.document.starting_zoos.iter().for_each(|record| {
            starts.insert(record.id, handle.clone());
        });
        asset.document.campaigns.iter().for_each(|record| {
            campaigns.insert(record.id, handle.clone());
        });
        if !asset.document.campaign_order.is_empty() {
            campaign_order.clone_from(&asset.document.campaign_order);
        }
        asset.document.scenarios.iter().for_each(|record| {
            scenario_records.insert(
                record.id,
                ScenarioDocumentHandle {
                    record: record.id,
                    handle: handle.clone(),
                },
            );
        });
        asset.campaign_scenarios.iter().for_each(|scenario| {
            if let Some(record) = assets
                .get(&scenario.handle)
                .and_then(|asset| asset.document.scenarios.first())
            {
                scenario_records.insert(
                    scenario.id,
                    ScenarioDocumentHandle {
                        record: record.id,
                        handle: scenario.handle.clone(),
                    },
                );
            }
        });
        asset.document.photo_challenges.iter().for_each(|record| {
            photo_challenges.insert(record.id, handle.clone());
        });
        asset
            .document
            .photo_challenge_sets
            .iter()
            .for_each(|record| {
                photo_sets.insert(record.id, handle.clone());
            });
        asset.terrains.iter().for_each(|dependency| {
            terrains.insert(dependency.id, handle.clone());
        });
        asset.textures.iter().for_each(|dependency| {
            textures.insert(dependency.id, handle.clone());
        });
        asset.starting_zoos.iter().for_each(|dependency| {
            start_paths.insert(dependency.id, dependency.path.clone());
        });
    }
    scenarios.locations = locations;
    scenarios.maps = maps;
    scenarios.starts = starts;
    scenarios.campaigns = campaigns;
    campaign_order.retain(|id| scenarios.campaigns.contains_key(id));
    for id in scenarios.campaigns.keys() {
        if !campaign_order.contains(id) {
            campaign_order.push(*id);
        }
    }
    scenarios.campaign_order = campaign_order;
    scenarios.scenarios = scenario_records;
    scenarios.photo_challenges = photo_challenges;
    scenarios.photo_sets = photo_sets;
    scenarios.terrains = terrains;
    scenarios.textures = textures;
    scenarios.start_paths = start_paths;
    scenarios.dirty = false;
}
