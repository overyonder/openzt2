//! Archive discovery and precedence indexing for world-scenario assets.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};

use crate::asset_source::{
    archive_entry_asset_path::archive_entry_asset_path, ArchiveCatalogue, ArchiveOverlayChanged,
    AssetArchives,
};

use super::{
    world_scenario_asset_set_state_and_borrowing_queries::{
        ScenarioDocumentHandle, WorldScenarios,
    },
    world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};

pub(crate) fn scenario_catalogue_path(path: &Path) -> bool {
    if !path
        .extension()
        .is_some_and(|extension| extension == "xml" || extension == "zt2")
    {
        return false;
    }
    let path = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    path.starts_with("locations/")
        || path.starts_with("photochall/")
        || path.starts_with("scenario/campaign/")
        // Challenge mode offers these by running each one's `validate`; no
        // other document references them.
        || path.starts_with("scenario/goals/challenge/")
        // The main menu's Challenge button selects this document for the
        // mode; its interface changes apply to every Challenge zoo.
        || path == "scenario/goals/challengegoals.xml"
        || matches!(path.as_str(), "maps/index.xml" | "maps/scenario/index.xml")
}

pub(super) fn refresh_world_scenario_catalogue_sources(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    mut scenarios: ResMut<WorldScenarios>,
    mut overlay_changes: MessageReader<ArchiveOverlayChanged>,
) {
    let changed = overlay_changes
        .read()
        .flat_map(|change| &change.changes)
        .any(|change| scenario_catalogue_path(&change.normalized_asset_path));
    overlay_changes.clear();
    if scenarios.discovered && !changed {
        return;
    }
    let sources = archives.catalogue_sources(ArchiveCatalogue::Scenarios);
    let handles = sources
        .iter()
        .map(|entry| server.load(archive_entry_asset_path(entry)))
        .collect::<Vec<_>>();
    if !scenarios.discovered || scenarios.handles != handles {
        scenarios.source_ids = handles.iter().map(Handle::id).collect();
        scenarios.handles = handles;
        scenarios.discovery_changed = true;
        scenarios.discovered = true;
    }
}

pub fn index_loaded_world_scenario_assets_by_identifier(
    server: Res<AssetServer>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut scenarios: ResMut<WorldScenarios>,
    mut events: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<WorldScenarioDocumentAsset>>,
) {
    let is_relevant =
        |id| scenarios.published_dependencies.contains(&id) || scenarios.source_ids.contains(&id);
    let mut touched = BTreeSet::new();
    let mut loaded = BTreeSet::new();
    let mut removed = BTreeSet::new();
    let mut failures_in_batch = BTreeSet::new();
    for event in events.read() {
        match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } if is_relevant(id) => {
                touched.insert(id);
                loaded.insert(id);
            }
            AssetEvent::Removed { id } if is_relevant(id) => {
                touched.insert(id);
                removed.insert(id);
            }
            _ => {}
        }
    }
    for failure in failures.read() {
        if is_relevant(failure.id) {
            failures_in_batch.insert(failure.id);
        }
    }
    if !scenarios.discovery_changed && touched.is_empty() && failures_in_batch.is_empty() {
        return;
    }
    let mut failed = std::mem::take(&mut scenarios.failed);
    let mut pending = std::mem::take(&mut scenarios.pending);
    for id in &touched {
        pending.remove(id);
    }
    for id in &loaded {
        failed.remove(id);
    }
    failed.extend(failures_in_batch.iter().copied());
    failed.extend(removed.difference(&loaded).copied());
    for id in &failures_in_batch {
        pending.remove(id);
    }
    let mut published_dependencies = BTreeSet::new();
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
        if failed.contains(&handle.id()) {
            continue;
        }
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
            let dependency = scenario.handle.id();
            published_dependencies.insert(dependency);
            if failed.contains(&dependency) {
                return;
            }
            if let Some(record) = assets
                .get(&scenario.handle)
                .and_then(|asset| asset.document.scenarios.first())
            {
                // Objectives retain the goal document's canonical scenario ID;
                // campaign selection addresses the same document by its alias.
                scenario_records.insert(
                    record.id,
                    ScenarioDocumentHandle {
                        record: record.id,
                        handle: scenario.handle.clone(),
                    },
                );
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
    scenarios.discovery_changed = false;
    let required = scenarios
        .handles
        .iter()
        .map(Handle::id)
        .chain(published_dependencies.iter().copied())
        .collect::<BTreeSet<_>>();
    pending.retain(|id| required.contains(id));
    failed.retain(|id| required.contains(id));
    for id in required.difference(&scenarios.required_ids).copied() {
        match server.get_load_state(id) {
            Some(LoadState::Failed(_)) => {
                failed.insert(id);
            }
            _ if !failed.contains(&id) && !assets.contains(id) => {
                pending.insert(id);
            }
            _ => {}
        }
    }
    scenarios.pending = pending;
    scenarios.failed = failed;
    scenarios.required_ids = required;
    scenarios.published_dependencies = published_dependencies;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_scenario::world_scenario_document_asset_and_dependency_handles::DemandLoadedAssetHandle;
    use openzt2_game_data::{
        world_scenario::{
            ScenarioCampaignRecord, ScenarioDefinitionRecord, ScenarioRecordFlags,
            WorldScenarioDocument,
        },
        AssetId,
    };

    #[test]
    fn campaign_index_publishes_its_dependency_records_only_after_the_dependency_arrives() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<WorldScenarioDocumentAsset>()
            .init_resource::<WorldScenarios>()
            .add_systems(Update, index_loaded_world_scenario_assets_by_identifier);
        let campaign_id = AssetId([1; 16]);
        let authored_key = AssetId([2; 16]);
        let record_id = AssetId([3; 16]);
        let dependency = app
            .world()
            .resource::<Assets<WorldScenarioDocumentAsset>>()
            .reserve_handle();
        let mut campaign = WorldScenarioDocumentAsset::from_test_document(WorldScenarioDocument {
            campaigns: vec![ScenarioCampaignRecord {
                id: campaign_id,
                name_key: campaign_id,
                description_key: campaign_id,
                scenarios: vec![],
            }],
            ..default()
        });
        campaign.campaign_scenarios = vec![DemandLoadedAssetHandle {
            id: authored_key,
            handle: dependency.clone(),
        }]
        .into_boxed_slice();
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
            .add(campaign);
        app.world_mut()
            .resource_mut::<WorldScenarios>()
            .queue_test_document(handle);
        app.update();
        let assets = app.world().resource::<Assets<WorldScenarioDocumentAsset>>();
        let scenarios = app.world().resource::<WorldScenarios>();
        // The campaign label remains browsable while its independently loaded
        // scenario is pending. Full-catalogue consumers cannot publish yet.
        assert!(scenarios
            .get(assets)
            .unwrap()
            .campaign(campaign_id)
            .is_some());
        assert!(scenarios
            .get(assets)
            .unwrap()
            .scenario(authored_key)
            .is_none());
        assert!(!scenarios.catalogue_documents_are_loaded(assets));
        app.world_mut()
            .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
            .insert(
                dependency.id(),
                WorldScenarioDocumentAsset::from_test_document(WorldScenarioDocument {
                    scenarios: vec![ScenarioDefinitionRecord {
                        id: record_id,
                        name_key: record_id,
                        description_key: record_id,
                        objectives: vec![],
                        objective_groups: vec![],
                        time_limit_ticks: 0,
                        evaluation_interval_seconds: 1.0,
                        flags: ScenarioRecordFlags::ALLOW_PAUSE,
                        ui_modifications: vec![],
                    }],
                    ..default()
                }),
            )
            .unwrap();
        app.update();
        app.update();
        let assets = app.world().resource::<Assets<WorldScenarioDocumentAsset>>();
        let scenarios = app.world().resource::<WorldScenarios>();
        assert!(scenarios.catalogue_documents_are_loaded(assets));
        assert_eq!(
            scenarios
                .get(assets)
                .unwrap()
                .scenario(authored_key)
                .unwrap()
                .id,
            record_id
        );
        let view = scenarios.get(assets).unwrap();
        assert_eq!(
            view.scenarios()
                .map(|scenario| scenario.id)
                .collect::<Vec<_>>(),
            vec![record_id]
        );
        let objective_scenario = view.scenario(authored_key).unwrap().id;
        let objective_document = view.handle_for_scenario(objective_scenario).unwrap();
        assert_eq!(objective_document, &dependency);
        assert_eq!(
            assets
                .get(objective_document)
                .unwrap()
                .document
                .find_scenario_record(objective_scenario)
                .unwrap()
                .id,
            record_id,
        );
    }
}
