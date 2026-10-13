use bevy::prelude::Assets;
use bevy::prelude::Commands;
use bevy::prelude::Entity;
use bevy::prelude::Name;
use bevy::prelude::Query;
use bevy::prelude::Res;
use bevy::prelude::ResMut;
use openzt2_game_data::world_scenario::StartingZooSpawnValue;
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::deterministic_random_stream::derive_zoo_seed_from_scenario_and_profile;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_zoo_state::progression::research_duration_calculation::resolve_research_duration_to_simulation_ticks;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_zoo_state::economy::facility_economy_types::Wallet;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::maintenance::maintenance_types::MaintainableObjectConditionPermille;
use openzt2_zoo_state::progression::research_types::ResearchProject;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;
use openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceAttribution;
use openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceStage;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

/// Validate all starting-zoo values before applying them to spawned entities.
pub(super) fn apply_all_authored_world_spawn_values_once(
    mut commands: Commands,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    allocator: Option<ResMut<PersistentIdAllocator>>,
    mut hydration: Query<(Entity, &mut WorldHydration, &SelectedWorldIdentity)>,
) {
    let _performance_timer =
        performance.measure(WorldLoadingPerformanceStage::AuthoredSpawnValueApplication);
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(mut allocator) = allocator else {
        return;
    };
    let Ok((root, mut hydration, selected_world)) = hydration.single_mut() else {
        return;
    };
    if !hydration.authored_world_spawn_value_application_is_pending() {
        return;
    }
    let Some(asset) = scenarios.get(hydration.starting_zoo_document_asset_handle()) else {
        hydration.record_failure(WorldLoadFailure::MissingScenario);
        return;
    };
    let catalog = &asset.document;
    let Some(start) = catalog.find_starting_zoo(hydration.starting_zoo_id()) else {
        hydration.record_failure(WorldLoadFailure::InvalidRecord(0));
        return;
    };

    let values = start.spawn_values.as_slice();
    let seed = start.imported_seed.map_or_else(
        || {
            derive_zoo_seed_from_scenario_and_profile(
                selected_world.requested,
                selected_world.profile,
            )
        },
        ZooSeed,
    );
    let mut research_value_count = 0_usize;
    for (index, value) in values.iter().enumerate() {
        match value {
            StartingZooSpawnValue::Name { entity, .. }
            | StartingZooSpawnValue::Condition { entity, .. }
            | StartingZooSpawnValue::CashRegister { entity, .. } => {
                if hydration
                    .spawned_world_prefab_root_at_record_index(*entity)
                    .is_none()
                {
                    hydration.record_failure(WorldLoadFailure::InvalidRecord(index as u32));
                    return;
                }
            }
            StartingZooSpawnValue::Research { item, progress } => {
                if *progress > 1_000 {
                    hydration.record_failure(WorldLoadFailure::InvalidRecord(index as u32));
                    return;
                }
                let definition = AssetId(item.0);
                if definitions.find_research(definition).is_none() {
                    if !active_definitions.is_complete() {
                        return;
                    }
                    hydration.record_failure(WorldLoadFailure::MissingDependency(definition));
                    return;
                }
                research_value_count += 1;
            }
        }
    }
    let allocator_has_capacity_for_every_research_value = u64::try_from(research_value_count)
        .ok()
        .and_then(|count| allocator.next().checked_add(count))
        .is_some();
    if !allocator.owns_world(root) || !allocator_has_capacity_for_every_research_value {
        hydration.record_failure(WorldLoadFailure::InvalidRecord(0));
        return;
    }

    for value in values {
        match value {
            StartingZooSpawnValue::Name { entity, text } => {
                let target = hydration
                    .spawned_world_prefab_root_at_record_index(*entity)
                    .expect("validated authored name target");
                commands.entity(target).insert(Name::new(text.clone()));
            }
            StartingZooSpawnValue::Condition { entity, permille } => {
                let target = hydration
                    .spawned_world_prefab_root_at_record_index(*entity)
                    .expect("validated authored condition target");
                commands
                    .entity(target)
                    .insert(MaintainableObjectConditionPermille(*permille));
            }
            StartingZooSpawnValue::CashRegister { entity, cents } => {
                let target = hydration
                    .spawned_world_prefab_root_at_record_index(*entity)
                    .expect("validated authored cash-register target");
                commands.entity(target).insert(Wallet(Money(*cents)));
            }
            StartingZooSpawnValue::Research { item, progress } => {
                let definition = AssetId(item.0);
                let research = definitions
                    .find_research(definition)
                    .expect("validated authored research definition");
                let id = allocator
                    .allocate(root)
                    .expect("validated persistent-ID allocator capacity");
                let required_ticks = resolve_research_duration_to_simulation_ticks(
                    research.duration,
                    definitions.timing().fixed_hz,
                    seed,
                    id,
                );
                let elapsed_ticks =
                    u64::try_from(u128::from(required_ticks) * u128::from(*progress) / 1_000)
                        .unwrap_or(required_ticks);
                commands.spawn((
                    ResearchProject {
                        definition,
                        elapsed_ticks,
                        required_ticks,
                    },
                    WorldMember { root },
                    id,
                ));
            }
        }
    }
    hydration.record_authored_world_spawn_value_application_completion();
}
