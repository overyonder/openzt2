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
        // Challenge mode offers these by running each one's `validate`; no
        // other document references them.
        || path.starts_with("scenario/goals/challenge/")
        // The main menu's Challenge button selects this document for the
        // mode; its interface changes apply to every Challenge zoo.
        || path == "scenario/goals/challengegoals.xml"
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

pub fn index_loaded_world_scenario_assets_by_identifier(
    server: Res<AssetServer>,
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
    // Partial indexes remain usable for browsing, but only a complete pass can
    // publish readiness for consumers that hydrate the catalogue's full contents.
    let pending_documents = scenarios.catalogue_document_handles(&assets).any(|handle| {
        !assets.contains(handle.id())
            && !matches!(
                server.load_state(handle.id()),
                bevy::asset::LoadState::Failed(_)
            )
    });
    scenarios.dirty = pending_documents;
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
            .init_resource::<Assets<WorldScenarioDocumentAsset>>()
            .init_resource::<WorldScenarios>()
            .add_message::<AssetEvent<WorldScenarioDocumentAsset>>()
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
                        flags: ScenarioRecordFlags::ALLOW_PAUSE,
                        ui_modifications: vec![],
                    }],
                    ..default()
                }),
            )
            .unwrap();
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
