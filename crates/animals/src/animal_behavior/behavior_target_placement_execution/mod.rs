//! Task frames of one `ZTBehPlaceTarget` instruction: choosing the object,
//! playing the placement set at the chosen site and continuing afterwards.

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use bevy::{
    ecs::{message::MessageCursor, relationship::RelationshipTarget},
    prelude::*,
};
use openzt2_game_assets::{
    behavior::behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection},
    scene_prefab::ScenePrefabAsset,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::behavior::{
    action::place_target::BehaviorPlacedTargetType, action_record::BehaviorAction,
};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

use super::behavior_target_placement_types as types;
use types::{
    BehaviorTargetPlacementMarkers, BehaviorTargetPlacementSiteOf, BehaviorTargetPlacementStage,
    PendingBehaviorTargetPlacement,
};

pub(super) fn invalidate_obsolete_behavior_target_placement_sites(
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    prefab_events: Option<Res<Messages<AssetEvent<ScenePrefabAsset>>>>,
    mut cursor: Local<MessageCursor<AssetEvent<ScenePrefabAsset>>>,
    markers: Query<&BehaviorTargetPlacementMarkers>,
    mut placements: Query<(
        Entity,
        &WorldMember,
        &mut BehaviorTaskExecutionState,
        &mut PendingBehaviorTargetPlacement,
    )>,
    mut commands: Commands,
) {
    let changed_prefabs = prefab_events
        .as_deref()
        .map(|events| {
            cursor
                .read(events)
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
    for (actor, world, mut task, mut pending) in &mut placements {
        if !pending.owns(&task, world) {
            continue;
        }
        let definition = pending.definition;
        if let BehaviorTargetPlacementStage::Siting { prefab } = &mut pending.stage {
            if definitions_changed {
                if let Some(catalogue) = catalogue {
                    let current = catalogue
                        .find_object(definition)
                        .and_then(|object| catalogue.scene(object.prefab));
                    if *prefab != current {
                        *prefab = current;
                    }
                }
            }
            continue;
        }
        let Some(site) = pending.site() else {
            continue;
        };
        if changed_prefabs.contains(&site.prefab.id())
            || (definitions_changed
                && catalogue.is_some_and(|catalogue| {
                    !site.uses_current_prefab(catalogue, pending.definition)
                }))
        {
            restore_placement_recipient(actor, &pending, &markers, &mut task);
            let next_action_tick = task.next_action_tick;
            task.restore_suspended_instruction(&pending.origin, pending.depth, next_action_tick);
            fail_placement(actor, &mut commands);
        }
    }
}

fn restore_placement_recipient(
    actor: Entity,
    pending: &PendingBehaviorTargetPlacement,
    markers: &Query<&BehaviorTargetPlacementMarkers>,
    task: &mut BehaviorTaskExecutionState,
) {
    if task.target.is_some_and(|target| {
        markers
            .get(actor)
            .is_ok_and(|markers| markers.iter().any(|marker| marker == target))
    }) {
        task.target = Some(pending.recipient);
        task.interaction_slot = None;
    }
}

pub(super) fn prepare_and_finish_behavior_target_placements(
    mut commands: Commands,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    loaded: Res<LoadedBehaviorDocumentCollection>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    animal_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    clock: Res<ZooClock>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &WorldMember,
            Option<&mut PendingBehaviorTargetPlacement>,
            Option<&SpeciesHandle>,
            Option<&StaffRole>,
            Option<&GuestArchetype>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    recipients: Query<
        (&WorldMember, &SpeciesHandle),
        (Without<Dead>, Without<WorldEntityIsCrated>),
    >,
    mut failing: Query<
        (
            Entity,
            &PendingBehaviorTargetPlacement,
            &mut BehaviorTaskExecutionState,
        ),
        With<PendingBehaviorTaskFailure>,
    >,
    markers: Query<&BehaviorTargetPlacementMarkers>,
    obsolete: Query<
        Entity,
        (
            With<PendingBehaviorTargetPlacement>,
            Without<PendingBehaviorTaskFailure>,
            Or<(Without<BehaviorTaskExecutionState>, Without<WorldMember>)>,
        ),
    >,
) {
    for actor in &obsolete {
        commands
            .entity(actor)
            .remove::<PendingBehaviorTargetPlacement>();
    }
    // A failing placement set leaves the task's failure outcome addressed to
    // the recipient, not to the site it was walking to.
    for (actor, pending, mut task) in &mut failing {
        restore_placement_recipient(actor, pending, &markers, &mut task);
        commands
            .entity(actor)
            .remove::<PendingBehaviorTargetPlacement>();
    }
    for (actor, mut task, world, pending, species, staff, guest) in &mut actors {
        if let Some(mut pending) = pending {
            if !pending.owns(&task, world) {
                // A replaced or interrupted task never receives the object.
                restore_placement_recipient(actor, &pending, &markers, &mut task);
                commands
                    .entity(actor)
                    .remove::<PendingBehaviorTargetPlacement>();
                continue;
            }
            if !recipients
                .get(pending.recipient)
                .is_ok_and(|(recipient_world, _)| recipient_world.root == pending.root)
            {
                restore_placement_recipient(actor, &pending, &markers, &mut task);
                fail_placement(actor, &mut commands);
                continue;
            }
            if !pending.is_current(&task) || task.next_action_tick > clock.tick {
                continue;
            }
            match pending.stage.clone() {
                BehaviorTargetPlacementStage::Siting { .. }
                | BehaviorTargetPlacementStage::PuttingDown { .. } => {}
                BehaviorTargetPlacementStage::Sited(site) => {
                    let Some(BehaviorAction::PlaceTarget(action)) = documents
                        .get(&task.document)
                        .and_then(|document| find_current_behavior_task_action(document, &task))
                    else {
                        continue;
                    };
                    let Some(view) = loaded.create_declaration_index_view(&documents) else {
                        continue;
                    };
                    let Some(catalogue) = definitions.get(&definition_assets) else {
                        continue;
                    };
                    let Some((document, declaration)) = view.find_behavior_set_location(
                        action.placement_behavior_set,
                        super::behavior_subject_type_resolution::behavior_subject_type_identifiers(
                            species,
                            staff,
                            guest,
                            catalogue,
                            animal_types.borrow_for(actor),
                        ),
                    ) else {
                        fail_placement(actor, &mut commands);
                        continue;
                    };
                    // The set returns to this instruction, which then puts the
                    // held object down at the site it walked to.
                    let frame = task.create_return_frame_after_action(task.action);
                    if !task.push_return_frame(frame) {
                        fail_placement(actor, &mut commands);
                        continue;
                    }
                    let marker = commands
                        .spawn((
                            site.transform,
                            // Docking may read the site before transform propagation.
                            GlobalTransform::from(site.transform),
                            BehaviorTargetPlacementSiteOf(actor),
                        ))
                        .id();
                    task.program = action.placement_behavior_set;
                    task.document = document;
                    task.declaration = declaration;
                    task.phase = BehaviorTaskExecutionPhase::Set;
                    task.action = 0;
                    task.repetitions = 0;
                    task.target = Some(marker);
                    task.interaction_slot = None;
                    task.next_action_tick = clock.tick.saturating_add(1);
                    pending.stage = BehaviorTargetPlacementStage::Delivering { site };
                }
                BehaviorTargetPlacementStage::Delivering { site } => {
                    task.target = Some(pending.recipient);
                    task.interaction_slot = None;
                    pending.stage = BehaviorTargetPlacementStage::PuttingDown {
                        site,
                        payment: types::BehaviorTargetPlacementPaymentState::Unrequested,
                    };
                }
                BehaviorTargetPlacementStage::Placed => {
                    commands
                        .entity(actor)
                        .remove::<PendingBehaviorTargetPlacement>();
                    advance_behavior_task_to_next_action(&mut task, clock.tick);
                }
                BehaviorTargetPlacementStage::Rejected => {
                    task.target = Some(pending.recipient);
                    fail_placement(actor, &mut commands);
                }
            }
            continue;
        }
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::PlaceTarget(action)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let Some(species_view) = species_index.get(&species_assets) else {
            continue;
        };
        // The target names the placed type through its own authored attribute.
        let definition = task
            .target
            .and_then(|target| recipients.get(target).ok())
            .filter(|(recipient_world, _)| recipient_world.root == world.root)
            .and_then(|(_, recipient)| species_view.find(recipient.species))
            .and_then(|recipient| match action.placed_type {
                BehaviorPlacedTargetType::KeeperFood => recipient.keeper_food_definition,
                BehaviorPlacedTargetType::KeeperDrink => recipient.keeper_drink_definition,
            });
        let (Some(definition), Some(recipient)) = (definition, task.target) else {
            fail_placement(actor, &mut commands);
            continue;
        };
        commands
            .entity(actor)
            .insert(PendingBehaviorTargetPlacement {
                execution_id: task.execution_id,
                origin: task.create_return_frame_after_action(task.action),
                depth: task.stack.len(),
                root: world.root,
                recipient,
                definition,
                stage: BehaviorTargetPlacementStage::Siting { prefab: None },
            });
    }
}

fn fail_placement(actor: Entity, commands: &mut Commands) {
    commands
        .entity(actor)
        .remove::<PendingBehaviorTargetPlacement>();
    mark_behavior_task_for_failure_and_stop_navigation(actor, commands);
}
