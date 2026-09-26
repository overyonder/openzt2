use std::collections::BTreeMap;

use bevy::prelude::{
    info, warn, AssetServer, Assets, Commands, Handle, Local, MessageReader, MessageWriter, Name,
    Query, Res, ResMut, With,
};
use openzt2_game_data::AssetId;

use crate::asset_source::AssetArchives;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::plugins::economy::guest_admission_types::AdmissionPrice;
use crate::plugins::economy::guest_admission_types::ZooAdmissionsOpen;
use crate::plugins::economy::money_types::Money;
use crate::plugins::economy::zoo_cash_types::ZooCash;
use crate::plugins::progression::fame_types::Fame;

use super::{
    persistent_id_types::PersistentIdAllocator,
    selected_world_identity::SelectedWorldIdentity,
    selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle,
    world_hydration_types::WorldHydration,
    world_load_failure::{WorldLoadFailed, WorldLoadFailure},
    world_loading_performance_attribution::{
        WorldLoadingPerformanceAttribution, WorldLoadingPerformanceStage,
    },
    world_membership_types::WorldRoot,
};

#[derive(bevy::prelude::Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BeginWorldLoad {
    pub(crate) scenario: AssetId,
    pub(crate) mode: crate::game_session_types::WorldSessionMode,
    pub(crate) profile: AssetId,
    pub(crate) starting_cash_cents: Option<i64>,
}

pub(super) fn accept_queued_world_load_after_required_assets_resolve(
    mut commands: Commands,
    archives: Res<AssetArchives>,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    mut requests: MessageReader<BeginWorldLoad>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    active_scenarios: Res<WorldScenarios>,
    asset_server: Res<AssetServer>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    roots: Query<(), With<WorldRoot>>,
    pending: Query<&WorldHydration>,
    mut failed: MessageWriter<WorldLoadFailed>,
    mut queued: Local<Option<BeginWorldLoad>>,
    mut demanded_starts: Local<BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>>,
) {
    for request in requests.read() {
        if queued.is_none() {
            performance.begin(request.scenario, &archives);
        }
        queued.get_or_insert(*request);
    }
    if performance.progress_report_is_due() {
        info!(
            target: "openzt2_scene_loading",
            definitions_ready = active_definitions.get(&definitions).is_some(),
            world_root_count = roots.iter().count(),
            hydration = ?pending.iter().next(),
            "scene loading pending owners"
        );
        for (start, handle) in demanded_starts.iter() {
            info!(
                target: "openzt2_scene_loading",
                ?start,
                path = ?asset_server.get_path(handle.id()),
                state = ?asset_server.get_load_states(handle.id()),
                "scene loading requested starting zoo"
            );
        }
    }
    let _performance_timer = performance.measure(WorldLoadingPerformanceStage::RequestAcceptance);
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(request) = *queued else {
        return;
    };
    if !roots.is_empty() || !pending.is_empty() {
        // Starting a load is idempotent while a world root or hydration
        // job already owns the transition. A second activation must not
        // turn a valid in-flight load into a failure.
        return;
    }

    let Some((
        map_asset,
        start_asset,
        map,
        terrain,
        start,
        record_count,
        cash_cents,
        admission_cents,
        fame_half_stars,
    )) = locate_selected_starting_zoo(
        &scenarios,
        &active_scenarios,
        &asset_server,
        &mut demanded_starts,
        request.scenario,
    )
    else {
        if demanded_starts.values().any(|handle| {
            matches!(
                asset_server.load_state(handle.id()),
                bevy::asset::LoadState::Failed(_)
            )
        }) {
            failed.write(WorldLoadFailed {
                scenario: request.scenario,
                reason: WorldLoadFailure::MissingScenario,
            });
            *queued = None;
            demanded_starts.clear();
            drop(_performance_timer);
            performance.finish_and_report(&archives, "failed");
        }
        return;
    };
    let Some(start_document) = scenarios.get(&start_asset) else {
        return;
    };
    let Some(start_record) = start_document.document.find_starting_zoo(start) else {
        failed.write(WorldLoadFailed {
            scenario: request.scenario,
            reason: WorldLoadFailure::MissingScenario,
        });
        return;
    };
    let zoo_name = &start_record.name;
    let Some(terrain_path) = scenarios
        .get(&map_asset)
        .and_then(|document| document.terrain_path(terrain))
    else {
        failed.write(WorldLoadFailed {
            scenario: request.scenario,
            reason: WorldLoadFailure::MissingDependency(terrain),
        });
        return;
    };
    let terrain_handle = asset_server.load::<TerrainAsset>(terrain_path.to_owned());
    let selection = SelectedWorldIdentity {
        requested: request.scenario,
        map,
        start,
        mode: request.mode,
        profile: request.profile,
    };
    let root = commands
        .spawn((
            WorldRoot {
                scenario: request.scenario,
            },
            Name::new(zoo_name.to_owned()),
            selection,
            SelectedWorldTerrainAssetHandle(terrain_handle),
        ))
        .id();
    commands.insert_resource(PersistentIdAllocator::new(root));
    info!(?root, scenario = ?request.scenario, "accepted world load and installed its persistent-id allocator");
    commands.insert_resource(ZooCash(Money(
        request.starting_cash_cents.unwrap_or(cash_cents),
    )));
    let admission_cents = admission_cents
        .or_else(|| {
            definitions
                .guest_generation()
                .map(|policy| i64::from(policy.normal_admission_cents[0]))
        })
        .unwrap_or_default();
    commands.insert_resource(AdmissionPrice(Money(admission_cents)));
    commands.insert_resource(ZooAdmissionsOpen(start_record.admissions_open));
    if start_record.maximum_fame_percent_reached.is_none()
        && matches!(
            request.mode,
            crate::game_session_types::WorldSessionMode::Campaign
                | crate::game_session_types::WorldSessionMode::Challenge
        )
        && definitions
            .research()
            .any(|research| research.minimum_fame_percent > 0.0)
    {
        warn!(
            "starting zoo has no maximum fame percentage; fame-gated research remains unavailable"
        );
    }
    commands.insert_resource(Fame {
        half_stars: fame_half_stars,
        maximum_reached: fame_half_stars,
        maximum_percent_reached: start_record.maximum_fame_percent_reached,
    });
    commands
        .entity(root)
        .insert(WorldHydration::new_pending_world_hydration(
            request.scenario,
            start_asset,
            start,
            record_count,
        ));
    *queued = None;
}

fn locate_selected_starting_zoo(
    scenarios: &Assets<WorldScenarioDocumentAsset>,
    active: &WorldScenarios,
    asset_server: &AssetServer,
    demanded_starts: &mut BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    scenario_id: AssetId,
) -> Option<(
    Handle<WorldScenarioDocumentAsset>,
    Handle<WorldScenarioDocumentAsset>,
    AssetId,
    AssetId,
    AssetId,
    usize,
    i64,
    Option<i64>,
    u8,
)> {
    active.get(scenarios).and_then(|view| {
        let (map_id, start_id, scenario_cash) =
            if let Some(scenario) = view.campaign_scenario(scenario_id) {
                (
                    AssetId(scenario.map.0),
                    AssetId(scenario.starting_zoo.0),
                    Some(scenario.starting_cash_cents),
                )
            } else if let Some(map) = view.map(scenario_id) {
                (scenario_id, AssetId(map.starting_zoo.0), None)
            } else {
                return None;
            };
        let map = view.map(map_id)?;
        let terrain_id = AssetId(map.terrain.0);
        let map_handle = view.handle_for_map(map_id)?.clone();
        let start_handle = view.handle_for_start(start_id).cloned().or_else(|| {
            let path = view.start_path(start_id)?;
            Some(
                demanded_starts
                    .entry(start_id)
                    .or_insert_with(|| asset_server.load(path.to_owned()))
                    .clone(),
            )
        })?;
        demanded_starts
            .entry(start_id)
            .or_insert_with(|| start_handle.clone());
        scenarios
            .get(&start_handle)
            .and_then(|asset| asset.document.find_starting_zoo(start_id))
            .map(|start| {
                (
                    map_handle,
                    start_handle,
                    map_id,
                    terrain_id,
                    start_id,
                    start.entities.len(),
                    scenario_cash.unwrap_or(start.cash_cents),
                    start.admission_cents,
                    start.fame_half_stars,
                )
            })
    })
}
