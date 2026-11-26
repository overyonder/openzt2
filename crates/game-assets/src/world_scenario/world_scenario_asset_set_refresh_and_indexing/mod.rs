//! Archive discovery and precedence indexing for world-scenario assets.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};

use openzt2_game_data::AssetId;

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

pub(super) type ScenarioSourceOrder = (usize, usize);
type ScenarioAssetId = bevy::asset::AssetId<WorldScenarioDocumentAsset>;

pub(super) struct ScenarioCatalogueIndex<T> {
    candidates: BTreeMap<AssetId, BTreeMap<ScenarioSourceOrder, T>>,
    source_keys: BTreeMap<ScenarioSourceOrder, BTreeSet<AssetId>>,
}

impl<T> Default for ScenarioCatalogueIndex<T> {
    fn default() -> Self {
        Self {
            candidates: BTreeMap::new(),
            source_keys: BTreeMap::new(),
        }
    }
}

impl<T> ScenarioCatalogueIndex<T> {
    pub(super) fn insert(&mut self, order: ScenarioSourceOrder, key: AssetId, value: T) {
        self.source_keys.entry(order).or_default().insert(key);
        self.candidates.entry(key).or_default().insert(order, value);
    }

    fn remove_source(&mut self, order: ScenarioSourceOrder) {
        for key in self.source_keys.remove(&order).into_iter().flatten() {
            if let std::collections::btree_map::Entry::Occupied(mut candidates) =
                self.candidates.entry(key)
            {
                candidates.get_mut().remove(&order);
                if candidates.get().is_empty() {
                    candidates.remove();
                }
            }
        }
    }

    pub(super) fn get(&self, key: &AssetId) -> Option<&T> {
        self.candidates
            .get(key)?
            .last_key_value()
            .map(|(_, value)| value)
    }

    pub(super) fn contains_key(&self, key: &AssetId) -> bool {
        self.candidates.contains_key(key)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &AssetId> {
        self.candidates.keys()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&AssetId, &T)> {
        self.candidates.iter().filter_map(|(key, candidates)| {
            candidates.last_key_value().map(|(_, value)| (key, value))
        })
    }
}

#[derive(Resource, Default, PartialEq, Eq)]
pub struct ScenarioCatalogueSources(
    BTreeMap<ScenarioSourceOrder, Handle<WorldScenarioDocumentAsset>>,
);

pub(super) fn refresh_world_scenario_catalogue_sources(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    mut sources: ResMut<ScenarioCatalogueSources>,
    mut overlay_changes: MessageReader<ArchiveOverlayChanged>,
) {
    let scenario_changed = overlay_changes
        .read()
        .flat_map(|change| &change.changes)
        .any(|change| scenario_catalogue_path(&change.normalized_asset_path));
    overlay_changes.clear();
    if !archives.is_added() && !scenario_changed {
        return;
    }
    sources.set_if_neq(ScenarioCatalogueSources(
        archives
            .catalogue_sources(ArchiveCatalogue::Scenarios)
            .iter()
            .map(|entry| {
                (
                    (entry.archive_priority_index, entry.zip_entry_ordinal),
                    server.load(archive_entry_asset_path(entry)),
                )
            })
            .collect(),
    ));
}

pub fn index_loaded_world_scenario_assets_by_identifier(
    server: Res<AssetServer>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    sources: Option<Res<ScenarioCatalogueSources>>,
    mut scenarios: ResMut<WorldScenarios>,
    mut events: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<WorldScenarioDocumentAsset>>,
) {
    let mut roots = BTreeSet::new();
    let mut changed_goals = BTreeSet::new();
    let mut campaign_order_changed = false;
    if let Some(sources) = sources.filter(|sources| sources.is_changed()) {
        let obsolete = scenarios
            .sources
            .iter()
            .filter_map(|(order, handle)| (sources.0.get(order) != Some(handle)).then_some(*order))
            .collect::<Vec<_>>();
        for order in obsolete {
            if let Some(handle) = scenarios.sources.remove(&order) {
                scenarios.source_orders.remove(&handle.id());
                changed_goals.insert(handle.id());
                roots.insert(order);
            }
        }
        for (&order, handle) in &sources.0 {
            if !scenarios.sources.contains_key(&order) {
                scenarios.sources.insert(order, handle.clone());
                scenarios.source_orders.insert(handle.id(), order);
                refresh_document_load_state(&mut scenarios, &server, &assets, handle.id());
                roots.insert(order);
            }
        }
        scenarios.catalogue_initialized = true;
    }

    #[cfg(any(test, feature = "testing"))]
    for handle in std::mem::take(&mut scenarios.queued_test_sources) {
        let order = (usize::MAX, scenarios.sources.len());
        scenarios.source_orders.insert(handle.id(), order);
        refresh_document_load_state(&mut scenarios, &server, &assets, handle.id());
        scenarios.sources.insert(order, handle);
        scenarios.catalogue_initialized = true;
        roots.insert(order);
    }

    let mut settled = BTreeSet::new();
    let mut removed = BTreeSet::new();
    for event in events.read() {
        match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                settled.insert(id);
            }
            AssetEvent::Removed { id } => {
                removed.insert(id);
                settled.insert(id);
            }
            _ => {}
        }
    }
    let failures = failures
        .read()
        .map(|failure| failure.id)
        .collect::<BTreeSet<_>>();
    settled.extend(failures.iter().copied());
    for id in settled {
        if !scenarios.source_orders.contains_key(&id) && !scenarios.goal_owners.contains_key(&id) {
            continue;
        }
        refresh_document_load_state(&mut scenarios, &server, &assets, id);
        if !assets.contains(id) && (removed.contains(&id) || failures.contains(&id)) {
            scenarios.pending.remove(&id);
            scenarios.failed.insert(id);
        }
        if let Some(order) = scenarios.source_orders.get(&id) {
            roots.insert(*order);
        }
        changed_goals.insert(id);
    }

    let mut scenario_sources = roots.clone();
    for order in roots {
        campaign_order_changed |= scenarios.campaign_orders.remove(&order).is_some()
            || scenarios.campaigns.source_keys.contains_key(&order);
        scenarios.locations.remove_source(order);
        scenarios.maps.remove_source(order);
        scenarios.starts.remove_source(order);
        scenarios.campaigns.remove_source(order);
        scenarios.photo_challenges.remove_source(order);
        scenarios.photo_sets.remove_source(order);
        scenarios.terrains.remove_source(order);
        scenarios.textures.remove_source(order);
        scenarios.start_paths.remove_source(order);
        if let Some(goals) = scenarios.goal_sources.remove(&order) {
            for goal in goals {
                if let std::collections::btree_map::Entry::Occupied(mut owners) =
                    scenarios.goal_owners.entry(goal)
                {
                    owners.get_mut().remove(&order);
                    if owners.get().is_empty() {
                        owners.remove();
                        changed_goals.insert(goal);
                    }
                }
            }
        }
        let Some(handle) = scenarios.sources.get(&order).cloned() else {
            continue;
        };
        let Some(asset) = assets
            .get(&handle)
            .filter(|_| !scenarios.failed.contains(&handle.id()))
        else {
            continue;
        };
        macro_rules! index {
            ($index:ident, $records:expr) => {
                for record in $records {
                    scenarios.$index.insert(order, record.id, handle.clone());
                }
            };
        }
        index!(locations, &asset.document.locations);
        index!(maps, &asset.document.maps);
        index!(starts, &asset.document.starting_zoos);
        index!(campaigns, &asset.document.campaigns);
        index!(photo_challenges, &asset.document.photo_challenges);
        index!(photo_sets, &asset.document.photo_challenge_sets);
        index!(terrains, &asset.terrains);
        index!(textures, &asset.textures);
        for dependency in &asset.starting_zoos {
            scenarios
                .start_paths
                .insert(order, dependency.id, dependency.path.clone());
        }
        if !asset.document.campaign_order.is_empty() {
            scenarios
                .campaign_orders
                .insert(order, asset.document.campaign_order.clone());
            campaign_order_changed = true;
        }
        campaign_order_changed |= !asset.document.campaigns.is_empty();
        for dependency in &asset.campaign_scenarios {
            let id = dependency.handle.id();
            if !scenarios.source_orders.contains_key(&id)
                && !scenarios.goal_owners.contains_key(&id)
            {
                refresh_document_load_state(&mut scenarios, &server, &assets, id);
                if !assets.contains(id) && (removed.contains(&id) || failures.contains(&id)) {
                    scenarios.pending.remove(&id);
                    scenarios.failed.insert(id);
                }
            }
            scenarios.goal_sources.entry(order).or_default().insert(id);
            scenarios.goal_owners.entry(id).or_default().insert(order);
        }
    }

    for id in changed_goals {
        if let Some(owners) = scenarios.goal_owners.get(&id) {
            scenario_sources.extend(owners);
        } else if !scenarios.source_orders.contains_key(&id) {
            scenarios.pending.remove(&id);
            scenarios.failed.remove(&id);
        }
    }
    let scenarios = &mut *scenarios;
    for order in scenario_sources {
        scenarios.scenarios.remove_source(order);
        let Some(handle) = scenarios.sources.get(&order) else {
            continue;
        };
        let Some(asset) = assets
            .get(handle)
            .filter(|_| !scenarios.failed.contains(&handle.id()))
        else {
            continue;
        };
        for record in &asset.document.scenarios {
            scenarios.scenarios.insert(
                order,
                record.id,
                ScenarioDocumentHandle {
                    record: record.id,
                    handle: handle.clone(),
                },
            );
        }
        for goal in &asset.campaign_scenarios {
            if scenarios.failed.contains(&goal.handle.id()) {
                continue;
            }
            if let Some(record) = assets
                .get(&goal.handle)
                .and_then(|asset| asset.document.scenarios.first())
            {
                for key in [record.id, goal.id] {
                    scenarios.scenarios.insert(
                        order,
                        key,
                        ScenarioDocumentHandle {
                            record: record.id,
                            handle: goal.handle.clone(),
                        },
                    );
                }
            }
        }
    }

    if campaign_order_changed {
        scenarios.campaign_order.clear();
        let mut authored = BTreeSet::new();
        if let Some((_, order)) = scenarios.campaign_orders.last_key_value() {
            for id in order {
                if scenarios.campaigns.contains_key(id) {
                    authored.insert(*id);
                    scenarios.campaign_order.push(*id);
                }
            }
        }
        scenarios.campaign_order.extend(
            scenarios
                .campaigns
                .keys()
                .filter(|id| !authored.contains(id))
                .copied(),
        );
    }
}

fn refresh_document_load_state(
    scenarios: &mut WorldScenarios,
    server: &AssetServer,
    assets: &Assets<WorldScenarioDocumentAsset>,
    id: ScenarioAssetId,
) {
    scenarios.pending.remove(&id);
    scenarios.failed.remove(&id);
    match server.get_load_state(id) {
        Some(LoadState::Failed(_)) => {
            scenarios.failed.insert(id);
        }
        _ if !assets.contains(id) => {
            scenarios.pending.insert(id);
        }
        _ => {}
    }
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
