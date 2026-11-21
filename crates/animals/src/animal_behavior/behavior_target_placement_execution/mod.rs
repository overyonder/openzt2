//! Choose the authored carried object, run its delivery set, then put it down.

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use bevy::prelude::*;
use openzt2_game_assets::{
    behavior::behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection},
    scene_prefab::ScenePrefabAsset,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::behavior::action::place_target::BehaviorPlacedTargetType;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::{world_entity_crating::WorldEntityIsCrated, world_membership_types::WorldMember},
};
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::SpeciesHandle,
    behavior_task_execution_types::{
        running_actions::{
            ResumeSuspendedBehaviorInstruction, RunBehaviorPlaceTarget, RunningBehaviorInstruction,
            SelectedBehaviorSet,
        },
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    guests::guest_simulation_types::GuestArchetype,
    staff::staff_employment_types::StaffRole,
};

use super::behavior_target_placement_types::{
    BehaviorPlacementDelivering, BehaviorPlacementOf, BehaviorPlacementPutDown,
    BehaviorPlacementRecipient, BehaviorPlacementResult, BehaviorPlacementSiting,
    BehaviorPlacementTravelSite, BehaviorPlacements, BehaviorTargetPlacement,
    BehaviorTargetPlacementSite,
};

pub(super) fn invalidate_obsolete_behavior_target_placement_sites(
    (definitions, definition_assets): (Res<WorldDefinitions>, Res<Assets<WorldDefinitionAsset>>),
    prefab_events: Option<MessageReader<AssetEvent<ScenePrefabAsset>>>,
    placements: Query<(
        Entity,
        &BehaviorPlacementOf,
        &BehaviorTargetPlacement,
        &BehaviorTargetPlacementSite,
    )>,
    actors: Query<(&BehaviorTaskExecutionState, &WorldMember)>,
    frames: openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames,
    mut commands: Commands,
) {
    let changed_prefabs = prefab_events
        .map(|mut events| {
            events
                .read()
                .filter_map(|event| match *event {
                    AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(id),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let definitions_changed = definitions.is_changed() || definition_assets.is_changed();
    if changed_prefabs.is_empty() && !definitions_changed {
        return;
    }
    let catalogue = definitions.get(&definition_assets);
    for (operation, owner, placement, site) in &placements {
        if changed_prefabs.contains(&site.prefab.id())
            || (definitions_changed
                && catalogue.is_some_and(|catalogue| {
                    !site.uses_current_prefab(catalogue, placement.definition)
                }))
        {
            let actor = owner.0;
            if let Ok((task, world)) = actors.get(actor) {
                if placement.owns(actor, task, world, &frames) {
                    if !placement.is_current(task) {
                        commands.trigger(ResumeSuspendedBehaviorInstruction {
                            actor,
                            execution_id: placement.execution_id,
                            origin: placement.origin.clone(),
                            depth: placement.depth,
                            instruction: placement.origin.clone(),
                            tick: task.next_action_tick,
                            advance: false,
                        });
                    }
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
            }
            commands.entity(operation).try_despawn();
        }
    }
}

pub(super) fn prepare_and_finish_behavior_target_placements(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    loaded: Res<LoadedBehaviorDocumentCollection>,
    (definitions, definition_assets): (Res<WorldDefinitions>, Res<Assets<WorldDefinitionAsset>>),
    species_index: Res<SpeciesAssets>,
    species_assets: Res<Assets<SpeciesAsset>>,
    animal_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    actors: Query<(
        &BehaviorTaskExecutionState,
        &WorldMember,
        Option<(&RunningBehaviorInstruction, &RunBehaviorPlaceTarget)>,
        Option<&SpeciesHandle>,
        Option<&StaffRole>,
        Option<&GuestArchetype>,
        Has<PendingBehaviorTaskFailure>,
    )>,
    new_actions: Query<Entity, (With<RunBehaviorPlaceTarget>, Without<BehaviorPlacements>)>,
    placements: Query<(
        Entity,
        &BehaviorPlacementOf,
        Option<&BehaviorPlacementRecipient>,
        &BehaviorTargetPlacement,
    )>,
    ready_sites: Query<
        &BehaviorTargetPlacementSite,
        (
            Without<BehaviorPlacementSiting>,
            Without<BehaviorPlacementDelivering>,
            Without<BehaviorPlacementPutDown>,
        ),
    >,
    delivering: Query<(), With<BehaviorPlacementDelivering>>,
    results: Query<&BehaviorPlacementResult>,
    frames: openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames,
    recipients: Query<
        (&WorldMember, &SpeciesHandle),
        (Without<Dead>, Without<WorldEntityIsCrated>),
    >,
) {
    for (operation, owner, recipient, placement) in &placements {
        let actor = owner.0;
        let Ok((task, world, step, species, staff, guest, failing)) = actors.get(actor) else {
            commands.entity(operation).try_despawn();
            continue;
        };
        if failing || !placement.owns(actor, task, world, &frames) {
            commands.entity(operation).try_despawn();
            continue;
        }
        if recipient
            .and_then(|recipient| recipients.get(recipient.0).ok())
            .is_none_or(|(recipient_world, _)| recipient_world.root != placement.root)
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            commands.entity(operation).try_despawn();
            continue;
        }
        let Some((instruction, action)) = step else {
            continue;
        };
        if !placement.is_current(task)
            || !instruction.is_current(task)
            || task.next_action_tick > clock.tick
        {
            continue;
        }
        if let Ok(result) = results.get(operation) {
            if result.0 {
                instruction.complete(actor, clock.tick, &mut commands);
            } else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            commands.entity(operation).try_despawn();
        } else if delivering.contains(operation) {
            commands
                .entity(actor)
                .remove::<BehaviorPlacementTravelSite>();
            instruction.clear_interaction_slot(actor, &mut commands);
            commands
                .entity(operation)
                .remove::<BehaviorPlacementDelivering>()
                .insert(BehaviorPlacementPutDown);
        } else if let Ok(site) = ready_sites.get(operation) {
            let (Some(view), Some(catalogue)) = (
                loaded.create_declaration_index_view(&documents),
                definitions.get(&definition_assets),
            ) else {
                continue;
            };
            let program = action.0.placement_behavior_set;
            let Some((document, declaration)) = view.find_behavior_set_location(
                program,
                super::behavior_subject_type_resolution::behavior_subject_type_identifiers(
                    species,
                    staff,
                    guest,
                    catalogue,
                    animal_types.borrow_for(actor),
                ),
            ) else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                commands.entity(operation).try_despawn();
                continue;
            };
            let point = commands
                .spawn((
                    site.transform,
                    GlobalTransform::from(site.transform),
                    ChildOf(operation),
                ))
                .id();
            commands
                .entity(actor)
                .insert(BehaviorPlacementTravelSite { operation, point });
            instruction.clear_interaction_slot(actor, &mut commands);
            instruction.enter_set(
                actor,
                SelectedBehaviorSet {
                    program,
                    document,
                    declaration,
                    tick: clock.tick,
                    repeat: Some(task.repetitions),
                    target: None,
                },
                &mut commands,
            );
            commands
                .entity(operation)
                .insert(BehaviorPlacementDelivering);
        }
    }

    for actor in &new_actions {
        let Ok((task, world, Some((instruction, action)), _, _, _, false)) = actors.get(actor)
        else {
            continue;
        };
        if task.next_action_tick > clock.tick || !instruction.is_current(task) {
            continue;
        }
        let Some(species_view) = species_index.get(&species_assets) else {
            continue;
        };
        let definition = task
            .target
            .and_then(|target| recipients.get(target).ok())
            .filter(|(recipient_world, _)| recipient_world.root == world.root)
            .and_then(|(_, recipient)| species_view.find(recipient.species))
            .and_then(|recipient| match action.0.placed_type {
                BehaviorPlacedTargetType::KeeperFood => recipient.keeper_food_definition,
                BehaviorPlacedTargetType::KeeperDrink => recipient.keeper_drink_definition,
            });
        let (Some(definition), Some(recipient)) = (definition, task.target) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        commands.spawn((
            BehaviorTargetPlacement {
                execution_id: task.execution_id,
                origin: task.create_return_frame_after_action(task.action),
                depth: task.depth,
                root: world.root,
                definition,
            },
            BehaviorPlacementOf(actor),
            BehaviorPlacementRecipient(recipient),
            BehaviorPlacementSiting::default(),
        ));
    }
}
