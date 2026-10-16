//! Execution of authored entity-container entry and behavior-set selection.

use bevy::prelude::*;
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
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
    mut occupancy: ResMut<super::interaction_container_occupancy::InteractionContainerOccupancy>,
    transforms: Query<&GlobalTransform>,
    children: Query<&Children>,
    attachment_transforms: super::behavior_docking_execution::NamedNodeTransforms,
    mut actors: Query<(
        Entity,
        &mut BehaviorTaskExecutionState,
        Option<&SpeciesHandle>,
        Option<&StaffRole>,
        Option<&GuestArchetype>,
        Option<&mut GuestRng>,
        Option<&mut BehaviorRandomStream>,
    )>,
) {
    let (Some(behavior_declarations), Some(world_definitions)) = (
        behavior_documents.create_declaration_index_view(&behavior_document_assets),
        active_world_definitions.get(&world_definition_assets),
    ) else {
        return;
    };
    for (actor, mut task, species, staff_role, guest, mut guest_random, mut behavior_random) in
        &mut actors
    {
        if task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(action) = behavior_document_assets
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        if !matches!(
            &action,
            BehaviorAction::EnterInteractionContainer { .. }
                | BehaviorAction::PlaySet(_)
                | BehaviorAction::RandomSet { .. }
        ) {
            continue;
        }
        let target_object = task.target().and_then(|target| {
            target_definitions
                .get(target)
                .ok()
                .and_then(|reference| world_definitions.find_object(reference.0))
        });
        let requested_behavior_set = match action {
            BehaviorAction::RandomSet {
                weighted_sets,
                minimum_plays,
                maximum_plays,
                looping,
            } => {
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
                    &mut task.repetitions,
                    random,
                ) {
                    Ok(Some(index)) => weighted_sets[index].0,
                    Ok(None) => {
                        advance_behavior_task_to_next_action(&mut task, simulation_clock.tick);
                        continue;
                    }
                    Err(()) => {
                        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                        continue;
                    }
                }
            }
            BehaviorAction::EnterInteractionContainer {
                container,
                entrance_behavior_set,
            } => {
                let Some(target) = task.target() else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let Some(target_object) = target_object else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let reservation_tag =
                    container.or_else(|| task.reservation_tag(&behavior_document_assets));
                let Ok(actor_transform) = transforms.get(actor) else {
                    continue;
                };
                let actor_position = actor_transform.translation();
                let selected_slot = occupancy.nearest_available_service_slot(
                    target, actor, &target_object.interaction_slots, reservation_tag,
                    |name| {
                        let position = if name.is_empty() {
                            transforms.get(target).ok()?.translation()
                        } else {
                            super::behavior_docking_execution::find_named_descendant_global_transform(
                                target, name, &children, &attachment_transforms,
                            )?.translation()
                        };
                        let offset = position - actor_position;
                        Some(Vec2::new(offset.x, offset.z))
                    },
                );
                let Some(slot_index) = selected_slot else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let slot = &target_object.interaction_slots[slot_index];
                if !occupancy.admit(target, actor, slot_index, slot.capacity) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                }
                task.interaction_slot = Some(slot_index);
                debug!(?actor, ?target, slot_index, entrance = ?slot.entrance_behavior_set,
                    already_docked = occupancy.is_docked(target, actor), "entered interaction container slot");
                // Native entry completes immediately for an already docked
                // member; replaying its entrance can restart a completed use.
                if occupancy.is_docked(target, actor) {
                    advance_behavior_task_to_next_action(&mut task, simulation_clock.tick);
                    continue;
                }
                entrance_behavior_set.unwrap_or(slot.entrance_behavior_set)
            }
            BehaviorAction::PlaySet(play_set) => {
                if play_set.behavior_set_asset_id == AssetId::from_key("usecontainer") {
                    let Some(target_object) = target_object else {
                        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                        continue;
                    };
                    let Some(slot) = task
                        .target()
                        .and_then(|target| occupancy.actor_slot(target, actor))
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
            );
        let Some((document, declaration)) = behavior_declarations
            .find_behavior_set_location(requested_behavior_set, subject_type_identifiers)
        else {
            debug!(?actor, set = ?requested_behavior_set, "behavior set has no declaration for this subject");
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let return_action = if matches!(action, BehaviorAction::RandomSet { .. }) {
            task.action
        } else {
            task.action.saturating_add(1)
        };
        let return_frame = task.create_return_frame_after_action(return_action);
        if !task.push_return_frame(return_frame) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        task.program = requested_behavior_set;
        task.document = document;
        task.declaration = declaration;
        task.phase = BehaviorTaskExecutionPhase::Set;
        task.action = 0;
        task.repetitions = 0;
        task.next_action_tick = simulation_clock.tick.saturating_add(1);
    }
}
