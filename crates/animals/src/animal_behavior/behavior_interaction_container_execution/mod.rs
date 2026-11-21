//! Execution of authored entity-container entry and behavior-set selection.

use super::behavior_container_admission::BehaviorAdmissionInstruction;
use bevy::prelude::*;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::requests::{
    InteractionContainerAdmissionOutcome, InteractionContainerAdmissionRequest,
};

use openzt2_game_data::AssetId;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorEnterInteractionContainer, RunBehaviorPlaySet, RunBehaviorRandomSet,
    RunningBehaviorInstruction, SelectedBehaviorSet,
};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerMemberships;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::guests::guest_simulation_types::GuestRng;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::behavior_random_stream_state::BehaviorRandomStream;
use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;

pub(super) fn enter_authored_interaction_containers_and_play_selected_behavior_sets(
    mut commands: Commands,
    behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    animal_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    simulation_clock: Res<ZooClock>,
    target_definitions: Query<&DefinitionId>,
    memberships: InteractionContainerMemberships,
    transforms: Query<&GlobalTransform>,
    children: Query<&Children>,
    attachment_transforms: super::behavior_docking_execution::NamedNodeTransforms,
    mut actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            (
                Option<&RunBehaviorEnterInteractionContainer>,
                Option<&RunBehaviorPlaySet>,
                Option<&RunBehaviorRandomSet>,
            ),
            Option<&SpeciesHandle>,
            Option<&StaffRole>,
            Option<&GuestArchetype>,
            Option<&mut GuestRng>,
            Option<&mut BehaviorRandomStream>,
        ),
        Or<(
            With<RunBehaviorEnterInteractionContainer>,
            With<RunBehaviorPlaySet>,
            With<RunBehaviorRandomSet>,
        )>,
    >,
) {
    let (Some(behavior_declarations), Some(world_definitions)) = (
        behavior_documents.create_declaration_index_view(&behavior_document_assets),
        active_world_definitions.get(&world_definition_assets),
    ) else {
        return;
    };
    for (
        actor,
        task,
        instruction,
        (entry, play, random_set),
        species,
        staff_role,
        guest,
        mut guest_random,
        mut behavior_random,
    ) in &mut actors
    {
        if task.next_action_tick > simulation_clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let mut remaining = task.repetitions;
        let target_object = task.target().and_then(|target| {
            target_definitions
                .get(target)
                .ok()
                .and_then(|reference| world_definitions.find_object(reference.0))
        });
        let requested_behavior_set = match (entry, play, random_set) {
            (
                _,
                _,
                Some(RunBehaviorRandomSet {
                    weighted_sets,
                    minimum_plays,
                    maximum_plays,
                    looping,
                }),
            ) => {
                let Some(random) = guest_random
                    .as_deref_mut()
                    .map(|random| &mut random.0)
                    .or_else(|| behavior_random.as_deref_mut().map(|random| &mut random.0))
                else {
                    // Actor hydration supplies its stream before behavior execution.
                    continue;
                };
                match super::behavior_random_choice_execution::select_next_authored_random_choice(
                    weighted_sets,
                    *minimum_plays,
                    *maximum_plays,
                    *looping,
                    &mut remaining,
                    random,
                ) {
                    Ok(Some(index)) => weighted_sets[index].0,
                    Ok(None) => {
                        instruction.complete(actor, simulation_clock.tick, &mut commands);
                        continue;
                    }
                    Err(()) => {
                        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                        continue;
                    }
                }
            }
            (
                Some(RunBehaviorEnterInteractionContainer {
                    container,
                    entrance_behavior_set,
                }),
                _,
                _,
            ) => {
                let Some(target) = task.target() else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let Some(target_object) = target_object else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let reservation_tag = container.or_else(|| instruction.reservation_tag());
                let Ok(actor_transform) = transforms.get(actor) else {
                    continue;
                };
                let actor_position = actor_transform.translation();
                let slots = memberships.service_slot_candidates(
                    target, actor, &target_object.interaction_slots, reservation_tag,
                    |name| {
                        let position = if name.is_empty() {
                            transforms.get(target).ok()?.translation()
                        } else {
                            super::behavior_docking_execution::find_named_descendant_global_transform(
                                target, name, &children, &attachment_transforms,
                            )?.translation()
                        };
                        Some((position - actor_position).xz())
                    },
                );
                let request = InteractionContainerAdmissionRequest::Admit {
                    container: target,
                    member: actor,
                    slots,
                    despawn_on_refusal: false,
                    queue_service_slots: Vec::new(),
                };
                let entrance_behavior_set = *entrance_behavior_set;
                let instruction = BehaviorAdmissionInstruction::new(actor, &task, instruction);
                commands.queue(move |world: &mut World| {
                    if instruction.is_current(world) {
                        let outcome = request.commit(world);
                        world.trigger(BehaviorContainerEntered {
                            actor,
                            target,
                            entrance_behavior_set,
                            outcome,
                        });
                    }
                });
                continue;
            }
            (_, Some(RunBehaviorPlaySet(play_set)), _) => {
                if play_set.behavior_set_asset_id == AssetId::from_key("usecontainer") {
                    let Some(target_object) = target_object else {
                        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                        continue;
                    };
                    let Some(slot) = task
                        .target()
                        .and_then(|target| memberships.actor_slot(target, actor))
                        .and_then(|slot| target_object.interaction_slots.get(slot))
                    else {
                        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                        continue;
                    };
                    slot.use_behavior_set
                } else {
                    play_set.behavior_set_asset_id
                }
            }
            _ => continue,
        };
        let subject_type_identifiers =
            super::behavior_subject_type_resolution::behavior_subject_type_identifiers(
                species,
                staff_role,
                guest,
                world_definitions,
                animal_types.borrow_for(actor),
            )
            .chain(target_definitions.get(actor).ok().into_iter().flat_map(
                |definition| {
                    std::iter::once(definition.0).chain(
                        world_definitions
                            .find_object(definition.0)
                            .into_iter()
                            .flat_map(|object| object.behavior_type_identifiers.iter().copied()),
                    )
                },
            ));
        let Some((document, declaration)) = behavior_declarations
            .find_behavior_set_location(requested_behavior_set, subject_type_identifiers)
        else {
            debug!(?actor, set = ?requested_behavior_set, "behavior set has no declaration for this subject");
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        instruction.enter_set(
            actor,
            SelectedBehaviorSet {
                program: requested_behavior_set,
                document,
                declaration,
                tick: simulation_clock.tick,
                repeat: random_set.map(|_| remaining),
                target: None,
            },
            &mut commands,
        );
    }
}

#[derive(EntityEvent)]
pub(super) struct BehaviorContainerEntered {
    #[event_target]
    actor: Entity,
    target: Entity,
    entrance_behavior_set: Option<AssetId>,
    outcome: InteractionContainerAdmissionOutcome,
}

pub(super) fn apply_interaction_container_entry(
    entered: On<BehaviorContainerEntered>,
    mut commands: Commands,
    behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    animal_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    clock: Res<ZooClock>,
    target_definitions: Query<&DefinitionId>,
    memberships: InteractionContainerMemberships,
    actors: Query<(
        &BehaviorTaskExecutionState,
        &RunningBehaviorInstruction,
        Option<&SpeciesHandle>,
        Option<&StaffRole>,
        Option<&GuestArchetype>,
    )>,
) {
    let actor = entered.actor;
    let Ok((task, instruction, species, staff, guest)) = actors.get(actor) else {
        return;
    };
    if !instruction.is_current(task) {
        return;
    }
    let InteractionContainerAdmissionOutcome::Granted { slot, .. } = entered.outcome else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    instruction.bind_interaction_slot(actor, slot, &mut commands);
    if memberships.is_docked(entered.target, actor) {
        instruction.complete(actor, clock.tick, &mut commands);
        return;
    }
    let (Some(behavior_declarations), Some(world_definitions)) = (
        behavior_documents.create_declaration_index_view(&behavior_document_assets),
        active_world_definitions.get(&world_definition_assets),
    ) else {
        return;
    };
    let entrance_set = entered.entrance_behavior_set.or_else(|| {
        let definition = target_definitions.get(entered.target).ok()?;
        Some(
            world_definitions
                .find_object(definition.0)?
                .interaction_slots
                .get(slot)?
                .entrance_behavior_set,
        )
    });
    let Some((set, (document, declaration))) = entrance_set.and_then(|set| {
        let subject_types =
            super::behavior_subject_type_resolution::behavior_subject_type_identifiers(
                species,
                staff,
                guest,
                world_definitions,
                animal_types.borrow_for(actor),
            );
        Some((
            set,
            behavior_declarations.find_behavior_set_location(set, subject_types)?,
        ))
    }) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    instruction.enter_set(
        actor,
        SelectedBehaviorSet {
            program: set,
            document,
            declaration,
            tick: clock.tick,
            repeat: None,
            target: None,
        },
        &mut commands,
    );
}
