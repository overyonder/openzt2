//! Live animal identity, ageing, reproduction, lineage, adoption and release.

mod adoption_offer_execution;
pub mod adoption_offer_types;
mod adoption_placement_execution;
use openzt2_zoo_state::animal_lifecycle::animal_adoption_contracts;
mod animal_adoption_execution;
mod animal_adoption_pricing;
mod animal_age_resolution;
mod animal_ageing_execution;
pub(crate) use animal_ageing_execution::ResolveAnimalLifeStages;
mod outgoing_animal_adoption_execution;
use openzt2_zoo_state::animal_lifecycle::animal_birth_and_release_contracts;
mod animal_birth_execution;
pub mod animal_entity_spawning;
pub mod animal_presentation_attachment;
mod animal_release_execution;
mod animal_spawn_operation_contracts;
mod animal_spawn_request_execution;
pub mod animal_spawn_resolution;
pub mod animal_spawn_resolution_types;
mod authored_entity_morph;
mod egg_incubation;
pub mod scripted_animal_spawning;
mod starting_animal_hydration;
use openzt2_zoo_state::animal_lifecycle::types;
mod ui_actions;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running;
use openzt2_zoo_state::construction::FixedConstructionSet;
use openzt2_zoo_state::placement::ObjectPlacementPreviewUpdateSet;

use animal_adoption_contracts::{
    AdoptAnimal, AnimalAdopted, BeginAnimalAdoptionPlacement, DeclineCurrentAnimalAdoptionOffers,
    PlaceAnimalForAdoption,
};
use animal_birth_and_release_contracts::{AnimalBorn, AnimalReleased, ReleaseAnimal};
use animal_spawn_operation_contracts::{AnimalSpawnRejected, AnimalSpawned, SpawnAnimalRequest};

pub struct AnimalLifecyclePlugin;

impl Plugin for AnimalLifecyclePlugin {
    fn build(&self, app: &mut App) {
        use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
            activated_behavior_actor, update_waiting_behavior_actors,
        };
        app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorSpawnChild>.pipe(animal_birth_execution::spawn_children_for_authored_births));
        app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorMorphEntity>.pipe(authored_entity_morph::morph_authored_entities));

        app.add_observer(authored_entity_morph::cancel_interrupted_morph_fade);
        app.add_observer(
            openzt2_zoo_state::animal_behavior::animal_lineage::end_live_animal_relationships,
        );
        adoption_offer_execution::register_adoption_offers(app);
        animal_ageing_execution::register_animal_ageing(app);
        app.add_systems(Update, (
            starting_animal_hydration::hydrate_starting_animals,
            crate::animal_welfare::systems::initialize_animal_needs
                .run_if(any_with_component::<openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed>),
            starting_animal_hydration::finish_starting_animal_hydration,
        ).chain().in_set(GameSet::Intent)
            .before(openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompletionSet)
            .run_if(in_state(GamePhase::Loading)));
        app.add_message::<AdoptAnimal>()
            .add_message::<BeginAnimalAdoptionPlacement>()
            .add_message::<animal_adoption_contracts::BeginAnimalRelocation>()
            .add_message::<AnimalAdopted>()
            .add_message::<DeclineCurrentAnimalAdoptionOffers>()
            .add_message::<PlaceAnimalForAdoption>()
            .add_message::<SpawnAnimalRequest>()
            .add_message::<AnimalSpawned>()
            .add_message::<AnimalSpawnRejected>()
            .add_message::<AnimalBorn>()
            .add_message::<ReleaseAnimal>()
            .add_message::<AnimalReleased>()
            .add_message::<scripted_animal_spawning::ScriptedAnimalSpawnRequest>()
            .add_systems(
                FixedUpdate,
                scripted_animal_spawning::spawn_scripted_animals
                    .in_set(FixedGameSet::Act)
                    .in_set(scripted_animal_spawning::ScriptedAnimalSpawnSet)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    (
                        ui_actions::consume_animal_ui_actions,
                        adoption_placement_execution::
                            begin_selected_species_adoption_placement_on_construction_cursor,
                        adoption_placement_execution::begin_animal_relocation_on_construction_cursor,
                        adoption_placement_execution::
                            validate_selected_species_adoption_placement_preview,
                        adoption_placement_execution::
                            confirm_selected_species_adoption_into_habitat,
                        adoption_placement_execution::
                            clear_animal_adoption_placement_after_construction_tool_change,
                    )
                        .chain()
                        .after(ObjectPlacementPreviewUpdateSet),
                    (
                        animal_presentation_attachment::
                            attach_selected_species_variant_models_to_animal_entities,
                        animal_presentation_attachment::
                            apply_authored_initial_animal_animation_repetition_policy_after_attachment,
                    )
                        .chain(),
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                FixedUpdate,
                animal_adoption_execution::request_animal_adoption_payment_or_unpaid_spawn
                    .in_set(FixedConstructionSet::RequestEconomy)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_adoption_execution::request_paid_animal_adoption_spawns_after_completed_transactions,
                    animal_adoption_execution::reject_animal_adoptions_after_rejected_transactions,
                    animal_adoption_execution::remove_settled_animal_adoption_refunds,
                )
                    .chain()
                    .in_set(FixedConstructionSet::CompleteEconomy)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                animal_spawn_request_execution::spawn_animals_requested_by_gameplay_operations
                    .in_set(FixedConstructionSet::Apply)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_adoption_execution::complete_animal_adoptions_after_successful_spawns,
                    animal_adoption_execution::refund_paid_animal_adoptions_after_rejected_spawns,
                )
                    .in_set(FixedConstructionSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    egg_incubation::initialize_live_eggs,
                    egg_incubation::index_egg_tasks.after(egg_incubation::initialize_live_eggs),
                    egg_incubation::update_egg_incubation.after(egg_incubation::initialize_live_eggs),
                    egg_incubation::start_idle_egg_tasks
                        .after(egg_incubation::index_egg_tasks)
                        .after(egg_incubation::update_egg_incubation)
                        .before(openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet::Think),
                )
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_is_running),
            )
            .add_systems(
                FixedUpdate,
                (
                    update_waiting_behavior_actors.pipe(animal_birth_execution::spawn_children_for_authored_births),
                    update_waiting_behavior_actors.pipe(authored_entity_morph::morph_authored_entities),
                    authored_entity_morph::remove_authored_morph_shells,
                ).in_set(openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_release_execution::release_eligible_adult_animals_from_the_zoo,
                    outgoing_animal_adoption_execution::transfer_animals_out_for_adoption,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod adoption_transaction_tests;
