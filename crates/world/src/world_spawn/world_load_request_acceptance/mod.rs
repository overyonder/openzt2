use bevy::ecs::{entity_disabling::Disabled, query::Allow};
use openzt2_game_assets::application_lifecycle::GamePhase;
use std::collections::BTreeMap;
use super::challenge_economy_initialization::{ChallengeEconomyInitializer, CHALLENGE_MODE_DOCUMENT};
use openzt2_world_state::game_session_types::WorldSessionMode;
use bevy::prelude::info;
use bevy::prelude::warn;
use bevy::prelude::AssetServer;
use bevy::prelude::Assets;
use bevy::prelude::{Commands, Component, DespawnOnExit, Entity};
use bevy::prelude::Handle;
use bevy::prelude::MessageReader;
use bevy::prelude::MessageWriter;
use bevy::prelude::Name;
use bevy::prelude::Query;
use bevy::prelude::Res;
use bevy::prelude::With;
use openzt2_game_data::AssetId;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_zoo_state::economy::guest_admission_types::AdmissionPrice;
use openzt2_zoo_state::economy::guest_admission_types::ZooAdmissionsOpen;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::zoo_cash_types::{UnlimitedZooCash, ZooCash};
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailed;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad;
use openzt2_world_state::world_spawn::world_load_request_acceptance::WorldLoadOperationOwner;

#[derive(Component)]
pub struct PendingWorldLoad {
    request: BeginWorldLoad,
    demanded_starts: BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    challenge_economy: Option<Handle<ChallengeEconomyInitializer>>,
}

pub fn admit_first_world_load_request(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut requests: MessageReader<BeginWorldLoad>,
    pending: Query<(), With<PendingWorldLoad>>,
) {
    let first = requests.read().next().copied();
    requests.clear();
    if let Some(request) = first.filter(|_| pending.is_empty()) {
        commands.spawn((
            PendingWorldLoad {
                request,
                demanded_starts: BTreeMap::new(),
                challenge_economy: (request.mode == WorldSessionMode::Challenge
                    && request.saved_world_document.is_none()
                    && request.starting_cash_cents.is_none())
                .then(|| asset_server.load(CHALLENGE_MODE_DOCUMENT)),
            },
            DespawnOnExit(GamePhase::Loading),
        ));
    }
}

pub fn accept_queued_world_load_after_required_assets_resolve(
    mut commands: Commands,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    challenge_initializers: Option<Res<Assets<ChallengeEconomyInitializer>>>,
    active_scenarios: Res<WorldScenarios>,
    asset_server: Res<AssetServer>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    roots: Query<(), (With<WorldRoot>, Allow<Disabled>)>,
    mut pending: Query<(Entity, &mut PendingWorldLoad)>,
    mut failed: MessageWriter<WorldLoadFailed>,
) {
    let Ok((load_entity, mut pending)) = pending.single_mut() else {
        return;
    };
    if !roots.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let request = pending.request;
    let challenge_cash = if let Some(handle) = pending.challenge_economy.as_ref() {
        if let Some(initializer) = challenge_initializers
            .as_ref()
            .and_then(|assets| assets.get(handle))
        {
            initializer.starting_cash_cents()
        } else if matches!(
            asset_server.load_state(handle.id()),
            bevy::asset::LoadState::Failed(_)
        ) {
            failed.write(WorldLoadFailed {
                operation: request.operation,
                root: None,
                scenario: request.scenario,
                reason: WorldLoadFailure::MissingDependency(AssetId::from_virtual_path(
                    CHALLENGE_MODE_DOCUMENT,
                )),
            });
            commands.entity(load_entity).despawn();
            return;
        } else {
            return;
        }
    } else {
        None
    };
    let demanded_starts = &mut pending.demanded_starts;
    let saved_document = request
        .saved_world_document
        .and_then(|identifier| asset_server.get_id_handle(identifier));
    if let Some(handle) = &saved_document {
        demanded_starts.insert(request.scenario, handle.clone());
    }
    if request.saved_world_document.is_some() && saved_document.is_none() {
        failed.write(WorldLoadFailed {
            operation: request.operation,
            root: None,
            scenario: request.scenario,
            reason: WorldLoadFailure::MissingScenario,
        });
        commands.entity(load_entity).despawn();
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
        demanded_starts,
        request.scenario,
        saved_document.as_ref(),
    )
    else {
        let failed_start = demanded_starts.values().any(|handle| {
            matches!(
                asset_server.load_state(handle.id()),
                bevy::asset::LoadState::Failed(_)
            )
        });
        let pending_start = demanded_starts.values().any(|handle| {
            !scenarios.contains(handle.id())
                && !matches!(
                    asset_server.load_state(handle.id()),
                    bevy::asset::LoadState::Failed(_)
                )
        });
        if failed_start
            || (!pending_start
                && (request.saved_world_document.is_some()
                    || !active_scenarios.has_pending_document_loads(&asset_server, &scenarios)))
        {
            failed.write(WorldLoadFailed {
                operation: request.operation,
                root: None,
                scenario: request.scenario,
                reason: WorldLoadFailure::MissingScenario,
            });
            commands.entity(load_entity).despawn();
        }
        return;
    };
    let Some(start_document) = scenarios.get(&start_asset) else {
        return;
    };
    let Some(start_record) = start_document.document.find_starting_zoo(start) else {
        failed.write(WorldLoadFailed {
            operation: request.operation,
            root: None,
            scenario: request.scenario,
            reason: WorldLoadFailure::MissingScenario,
        });
        commands.entity(load_entity).despawn();
        return;
    };
    if start_record.next_persistent_identifier == Some(0) {
        failed.write(WorldLoadFailed {
            operation: request.operation,
            root: None,
            scenario: request.scenario,
            reason: WorldLoadFailure::InvalidRecord(0),
        });
        commands.entity(load_entity).despawn();
        return;
    }
    let zoo_name = &start_record.name;
    let terrain_handle = scenarios.get(&map_asset).and_then(|document| {
        document.restored_terrain(terrain).cloned().or_else(|| {
            let source = document.terrain_path(terrain)?;
            Some(asset_server.load::<TerrainAsset>(source.to_owned()))
        })
    });
    let Some(terrain_handle) = terrain_handle else {
        failed.write(WorldLoadFailed {
            operation: request.operation,
            root: None,
            scenario: request.scenario,
            reason: WorldLoadFailure::MissingDependency(terrain),
        });
        commands.entity(load_entity).despawn();
        return;
    };
    let selection = SelectedWorldIdentity {
        requested: request.scenario,
        map,
        start,
        mode: request.mode,
        profile: request.profile,
    };
    let starting_cash = ZooCash(Money(if request.saved_world_document.is_some() {
        start_record.cash_cents
    } else {
        request
            .starting_cash_cents
            .or(challenge_cash)
            .unwrap_or(cash_cents)
    }));
    let general_accounts = start_record.general_cash_cents.map(|cash| {
        GeneralEconomyLedger::from_starting_balance(
            u32::from(start_record.calendar[0]) * 12
                + u32::from(start_record.calendar[1].saturating_sub(1)),
            Money(cash),
        )
    });
    let starting_admission = admission_cents
        .map(|cents| AdmissionPrice::from_adult(Money(cents)))
        .or_else(|| {
            definitions.guest_generation().map(|policy| {
                let [adult, child] = policy
                    .normal_admission_cents
                    .map(|cents| Money(i64::from(cents)));
                AdmissionPrice { adult, child }
            })
        })
        .unwrap_or_default();
    let root = commands
        .spawn((
            WorldRoot {
                scenario: request.scenario,
            },
            Name::new(zoo_name.to_owned()),
            selection,
            openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles {
                map: map_asset.clone(),
                start: start_asset.clone(),
            },
            SelectedWorldTerrainAssetHandle(terrain_handle),
            starting_cash,
            starting_admission,
            ZooAdmissionsOpen(start_record.admissions_open),
            UnlimitedZooCash(request.mode == openzt2_world_state::game_session_types::WorldSessionMode::Freeform),
        ))
        .id();
    if let Some(general_accounts) = general_accounts {
        commands.entity(root).insert(general_accounts);
    }
    if let Some(operation) = request.operation {
        commands
            .entity(root)
            .insert(WorldLoadOperationOwner(operation));
    }
    let allocator = match start_record.next_persistent_identifier {
        Some(next) => PersistentIdAllocator::from_snapshot(next)
            .expect("validated starting persistent identifier"),
        None => PersistentIdAllocator::default(),
    };
    commands.entity(root).insert(allocator);
    info!(?root, scenario = ?request.scenario, "accepted world load and installed its persistent-id allocator");
    let mode_path = match request.mode {
        openzt2_world_state::game_session_types::WorldSessionMode::Freeform => {
            "config/freeform_mode.xml"
        }
        openzt2_world_state::game_session_types::WorldSessionMode::Challenge => {
            "config/challenge_mode.xml"
        }
        openzt2_world_state::game_session_types::WorldSessionMode::Campaign => {
            "config/campaign_mode.xml"
        }
    };
    let starting_fame = definitions.starting_fame(AssetId::from_virtual_path(mode_path));
    let fame_half_stars = starting_fame.map_or(fame_half_stars, |policy| {
        (policy.percent / 10.0).floor() as u8
    });
    if starting_fame.is_none()
        && start_record.maximum_fame_percent_reached.is_none()
        && matches!(
            request.mode,
            openzt2_world_state::game_session_types::WorldSessionMode::Campaign
                | openzt2_world_state::game_session_types::WorldSessionMode::Challenge
        )
        && definitions
            .research()
            .any(|research| research.minimum_fame_percent > 0.0)
    {
        warn!(
            "starting zoo has no maximum fame percentage; fame-gated research remains unavailable"
        );
    }
    info!(fame_half_stars, starting_fame_percent = ?starting_fame.map(|policy| policy.percent), "initialized starting zoo fame");
    commands.entity(root).insert(Fame {
        current_percent: starting_fame.map(|policy| policy.percent),
        half_stars: fame_half_stars,
        maximum_reached: fame_half_stars,
        maximum_percent_reached: starting_fame
            .map(|policy| policy.percent)
            .or(start_record.maximum_fame_percent_reached),
    });
    commands
        .entity(root)
        .insert(WorldHydration::new_pending_world_hydration(record_count));
    commands.entity(load_entity).despawn();
}

fn locate_selected_starting_zoo(
    scenarios: &Assets<WorldScenarioDocumentAsset>,
    active: &WorldScenarios,
    asset_server: &AssetServer,
    demanded_starts: &mut BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    scenario_id: AssetId,
    saved_document: Option<&Handle<WorldScenarioDocumentAsset>>,
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
    if let Some(handle) = saved_document {
        let document = &scenarios.get(handle)?.document;
        let (map_id, start_id, cash) = if let Some(map) = document.find_world_map(scenario_id) {
            (map.id, map.starting_zoo, None)
        } else {
            let scenario = document
                .campaigns
                .iter()
                .flat_map(|campaign| &campaign.scenarios)
                .find(|scenario| scenario.id == scenario_id)?;
            (
                scenario.map,
                scenario.starting_zoo,
                Some(scenario.starting_cash_cents),
            )
        };
        let map = document.find_world_map(map_id)?;
        let start = document.find_starting_zoo(start_id)?;
        return Some((
            handle.clone(),
            handle.clone(),
            map_id,
            map.terrain,
            start_id,
            start.entities.len(),
            cash.unwrap_or(start.cash_cents),
            start.admission_cents,
            start.fame_half_stars,
        ));
    }
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
