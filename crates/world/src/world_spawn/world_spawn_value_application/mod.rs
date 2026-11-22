use bevy::prelude::Assets;
use bevy::prelude::Commands;
use bevy::prelude::Entity;
use bevy::prelude::Name;
use bevy::prelude::Query;
use bevy::prelude::Res;
use bevy::prelude::With;
use bevy::prelude::Without;
use openzt2_game_data::world_scenario::StartingZooSpawnValue;
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_load_completion_marker::{
    StartingPrefabsHydrated, StartingSpawnValuesApplied,
};
use openzt2_zoo_state::progression::research_duration_calculation::resolve_research_duration_to_simulation_ticks;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_zoo_state::economy::facility_economy_types::Wallet;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::progression::research_types::ResearchProject;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_entity_crating::AnimalCrate;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

/// Validate all starting-zoo values before applying them to spawned entities.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_all_authored_world_spawn_values_once(
    mut commands: Commands,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    seed: Option<Res<ZooSeed>>,
    mut names: Query<&mut Name>,
    mut wallets: Query<&mut Wallet>,
    mut pending: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &SelectedWorldDocumentHandles,
            &mut PersistentIdAllocator,
            &WorldHydration,
        ),
        (
            With<StartingPrefabsHydrated>,
            Without<StartingSpawnValuesApplied>,
            Without<WorldLoadFailure>,
        ),
    >,
    starting_animals: Query<
        (),
        bevy::prelude::Or<(
            bevy::prelude::With<
                openzt2_world_state::world_spawn::starting_animal_hydration::PendingStartingAnimal,
            >,
            bevy::prelude::With<openzt2_zoo_state::animal_lifecycle::types::Animal>,
        )>,
    >,
) {
    let (Some(definitions), Some(seed)) = (active_definitions.get(&definitions), seed) else {
        return;
    };
    let Ok((root, selected_world, documents, mut allocator, hydration)) = pending.single_mut()
    else {
        return;
    };
    let Some(asset) = scenarios.get(&documents.start) else {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::MissingScenario);
        return;
    };
    let catalog = &asset.document;
    let Some(start) = catalog.find_starting_zoo(selected_world.start) else {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::InvalidRecord(0));
        return;
    };

    let values = start.spawn_values.as_slice();
    let mut research_value_count = 0_usize;
    for (index, value) in values.iter().enumerate() {
        match value {
            StartingZooSpawnValue::Name { entity, .. }
            | StartingZooSpawnValue::CashRegister { entity, .. } => {
                if hydration
                    .spawned_world_prefab_root_at_record_index(*entity)
                    .is_none()
                {
                    commands
                        .entity(root)
                        .insert_if_new(WorldLoadFailure::InvalidRecord(index as u32));
                    return;
                }
            }
            StartingZooSpawnValue::Research { item, progress } => {
                if *progress > 1_000 {
                    commands
                        .entity(root)
                        .insert_if_new(WorldLoadFailure::InvalidRecord(index as u32));
                    return;
                }
                let definition = AssetId(item.0);
                if definitions.find_research(definition).is_none() {
                    if !active_definitions.is_complete() {
                        return;
                    }
                    commands
                        .entity(root)
                        .insert_if_new(WorldLoadFailure::MissingDependency(definition));
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
    if !allocator_has_capacity_for_every_research_value {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::InvalidRecord(0));
        return;
    }

    let mut animals_with_crate = bevy::ecs::entity::EntityHashSet::new();
    let mut crate_links = Vec::new();
    for (index, record) in start.entities.iter().enumerate() {
        let mut occupant = None;
        for &contained in &record.contained_entities {
            let Some(animal) = hydration.spawned_world_prefab_root_at_record_index(contained)
            else {
                commands
                    .entity(root)
                    .insert_if_new(WorldLoadFailure::InvalidRecord(contained));
                return;
            };
            if starting_animals.get(animal).is_err() {
                continue;
            }
            if index == contained as usize
                || occupant.is_some()
                || !animals_with_crate.insert(animal)
            {
                commands
                    .entity(root)
                    .insert_if_new(WorldLoadFailure::InvalidRecord(index as u32));
                return;
            }
            occupant = Some(animal);
        }
        if let Some(animal) = occupant {
            let crate_entity = hydration
                .spawned_world_prefab_root_at_record_index(index as u32)
                .expect("starting prefab completion provides all source rows");
            crate_links.push((animal, crate_entity));
        }
    }
    for (animal, crate_entity) in crate_links {
        commands.entity(animal).insert(AnimalCrate(crate_entity));
    }
    for value in values {
        match value {
            StartingZooSpawnValue::Name { entity, text } => {
                let target = hydration
                    .spawned_world_prefab_root_at_record_index(*entity)
                    .expect("validated authored name target");
                if let Ok(mut name) = names.get_mut(target) {
                    if name.as_str() != text {
                        name.set(text.clone());
                    }
                } else {
                    commands.entity(target).insert(Name::new(text.clone()));
                }
            }
            StartingZooSpawnValue::CashRegister { entity, cents } => {
                let target = hydration
                    .spawned_world_prefab_root_at_record_index(*entity)
                    .expect("validated authored cash-register target");
                if let Ok(mut wallet) = wallets.get_mut(target) {
                    if wallet.0 != Money(*cents) {
                        wallet.0 = Money(*cents);
                    }
                } else {
                    commands.entity(target).insert(Wallet(Money(*cents)));
                }
            }
            StartingZooSpawnValue::Research { item, progress } => {
                let definition = AssetId(item.0);
                let research = definitions
                    .find_research(definition)
                    .expect("validated authored research definition");
                let id = allocator
                    .allocate()
                    .expect("validated persistent-ID allocator capacity");
                let required_ticks = resolve_research_duration_to_simulation_ticks(
                    research.duration,
                    definitions.timing().fixed_hz,
                    *seed,
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
    commands.entity(root).insert(StartingSpawnValuesApplied);
}
