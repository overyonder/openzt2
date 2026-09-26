use bevy::prelude::*;

#[cfg(test)]
mod navigation_event_tests;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::behavior_task_execution_types::advance_behavior_task_to_next_action;
use crate::plugins::behavior_task_execution_types::find_current_behavior_task_action;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::plugins::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use crate::plugins::behavior_task_execution_types::PendingBehaviorTaskFailure;
use crate::plugins::locomotion::locomotion_types::Arrived;
use crate::plugins::locomotion::locomotion_types::DockAt;
use crate::plugins::locomotion::locomotion_types::NavAgent;
use crate::plugins::locomotion::locomotion_types::NavigationFailed;
use crate::plugins::placement::placed_object_types::PlacedObjectAuthoredEntrance;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

pub(super) fn start_current_supported_behavior_docking_actions(
    mut commands: Commands,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<
        (Entity, &NavAgent, &mut BehaviorTaskExecutionState),
        (
            Without<PendingBehaviorDockingCompletion>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    target_entrances: Query<&PlacedObjectAuthoredEntrance>,
    target_definition_references: Query<&DefinitionId>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    children: Query<&Children>,
    authored_attachment_transforms: Query<(&PrefabAuthoredAttachmentIdentifier, &GlobalTransform)>,
    entity_global_transforms: Query<&GlobalTransform>,
    mut navigation_request_sequence: ResMut<
        crate::plugins::locomotion::locomotion_types::NavigationRequestSequence,
    >,
    mut docking_requests: MessageWriter<DockAt>,
    mut occupancy: ResMut<super::interaction_container_occupancy::InteractionContainerOccupancy>,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    for (actor, navigation_agent, mut behavior_task) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(BehaviorAction::Dock(docking_action)) = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        else {
            continue;
        };
        let Some(target) = behavior_task.target() else {
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                actor, &mut commands,
            );
            continue;
        };
        if !entity_global_transforms.contains(target) {
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                actor, &mut commands,
            );
            continue;
        }
        let target_definition = world_definitions.and_then(|definitions| {
            target_definition_references
                .get(target)
                .ok()
                .and_then(|reference| definitions.find_object(reference.0))
        });
        if let Some(reservation_tag) = behavior_task.reservation_tag(&behavior_document_assets) {
            let Some(definition) = target_definition else {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                    actor, &mut commands,
                );
                continue;
            };
            let Ok(actor_transform) = entity_global_transforms.get(actor) else {
                continue;
            };
            let selected = occupancy.nearest_available_service_slot(
                target,
                actor,
                &definition.interaction_slots,
                Some(reservation_tag),
                |name| {
                    let position = if name.is_empty() {
                        entity_global_transforms.get(target).ok()?.translation()
                    } else {
                        find_named_descendant_global_transform(
                            target,
                            name,
                            &children,
                            &authored_attachment_transforms,
                        )?
                        .translation()
                    };
                    let offset = position - actor_transform.translation();
                    Some(Vec2::new(offset.x, offset.z))
                },
            );
            let Some(slot_index) = selected else {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                    actor, &mut commands,
                );
                continue;
            };
            if !occupancy.admit(
                target,
                actor,
                slot_index,
                definition.interaction_slots[slot_index].capacity,
            ) {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                    actor, &mut commands,
                );
                continue;
            }
            behavior_task.interaction_slot = Some(slot_index);
        }
        if !docking_action.redock && occupancy.is_docked(target, actor) {
            advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
            continue;
        }
        let interaction_slot_target_node_name = target_definition
            .and_then(|definition| {
                occupancy
                    .actor_slot(target, actor)
                    .and_then(|slot| definition.interaction_slots.get(slot))
            })
            .map(|slot| slot.target_node_name.as_str())
            .filter(|name| !name.is_empty());
        let target_node_name = docking_action
            .target_node_name
            .as_deref()
            .or(interaction_slot_target_node_name);
        let authored_named_target_dock = target_node_name.and_then(|target_node_name| {
            find_named_descendant_global_transform(
                target,
                target_node_name,
                &children,
                &authored_attachment_transforms,
            )
            .and_then(|target_node_global_transform| {
                let target_global_transform = entity_global_transforms.get(target).ok()?;
                let target_from_node = target_global_transform.affine().inverse()
                    * target_node_global_transform.affine();
                Some((
                    target_from_node.translation.into(),
                    target_from_node
                        .transform_vector3(Vec3::Z)
                        .normalize_or_zero(),
                ))
            })
        });
        if target_node_name.is_some() && authored_named_target_dock.is_none() {
            continue;
        }
        let (mut local_point, local_forward) = authored_named_target_dock.unwrap_or_else(|| {
            target_entrances
                .get(target)
                .map_or((Vec3::ZERO, Vec3::Z), |entrance| {
                    (entrance.local_position, entrance.local_forward)
                })
        });
        if let Some(subject_node_name) = docking_action.subject_node_name.as_deref() {
            let Some(subject_node_global_transform) = find_named_descendant_global_transform(
                actor,
                subject_node_name,
                &children,
                &authored_attachment_transforms,
            ) else {
                continue;
            };
            let Some(actor_global_transform) = entity_global_transforms.get(actor).ok() else {
                continue;
            };
            let subject_node_local_position = actor_global_transform
                .affine()
                .inverse()
                .transform_point3(subject_node_global_transform.translation());
            let Ok(target_global_transform) = entity_global_transforms.get(target) else {
                continue;
            };
            local_point = align_subject_attachment_with_target_dock(
                local_point,
                local_forward,
                subject_node_local_position,
                actor_global_transform,
                target_global_transform,
            );
        }
        occupancy.begin_redocking(actor);
        let request_id = navigation_request_sequence.next();
        docking_requests.write(DockAt {
            entity: actor,
            request_id,
            target,
            local_point,
            local_forward,
            radius_m: navigation_agent.radius_m.max(0.05),
        });
        commands
            .entity(actor)
            .insert(PendingBehaviorDockingCompletion {
                target,
                origin: behavior_task.create_return_frame_after_action(behavior_task.action),
                stack_depth: behavior_task.stack.len(),
                request_id,
            });
        behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
    }
}

fn align_subject_attachment_with_target_dock(
    local_point: Vec3,
    local_forward: Vec3,
    subject_node_local_position: Vec3,
    actor_transform: &GlobalTransform,
    target_transform: &GlobalTransform,
) -> Vec3 {
    // Native docking subtracts the subject attachment after applying the
    // subject scale and final world orientation. DockAt instead takes a point
    // in target-local space: convert the offset back through that target,
    // including its independent scale, only after doing this world-space sum.
    let target_affine = target_transform.affine();
    let world_forward = target_affine.transform_vector3(local_forward);
    let horizontal_forward = Vec2::new(world_forward.x, world_forward.z).normalize_or_zero();
    let rotation = if horizontal_forward == Vec2::ZERO {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_y(horizontal_forward.x.atan2(horizontal_forward.y))
    };
    let (actor_scale, _, _) = actor_transform.to_scale_rotation_translation();
    let world_offset = rotation * (actor_scale * subject_node_local_position);
    local_point - target_affine.inverse().transform_vector3(world_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_dock_offset_preserves_independent_actor_and_facility_scales() {
        let actor = GlobalTransform::from(Transform::from_scale(Vec3::splat(0.5)));
        let target = GlobalTransform::from(Transform {
            translation: Vec3::new(10.0, 2.0, 20.0),
            rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            scale: Vec3::splat(3.0),
        });
        let socket = Vec3::new(0.0, 0.0, 2.0);
        let point =
            align_subject_attachment_with_target_dock(socket, Vec3::Z, Vec3::Z, &actor, &target);
        let subject_attachment_world = target.transform_point(point) + Vec3::X * 0.5;
        assert!(subject_attachment_world.abs_diff_eq(target.transform_point(socket), 1.0e-5));
    }
}

pub(super) fn find_named_descendant_global_transform<'a>(
    root: Entity,
    requested_name: &str,
    children: &Query<&Children>,
    authored_attachment_transforms: &'a Query<(
        &PrefabAuthoredAttachmentIdentifier,
        &GlobalTransform,
    )>,
) -> Option<&'a GlobalTransform> {
    let requested_attachment_identifier =
        openzt2_game_data::AssetId::from_key(&requested_name.trim().to_ascii_lowercase());
    children
        .iter_descendants_depth_first::<Children>(root)
        .find_map(|descendant| {
            authored_attachment_transforms
                .get(descendant)
                .ok()
                .filter(|(identifier, _)| identifier.0 == requested_attachment_identifier)
                .map(|(_, transform)| transform)
        })
}

pub(super) fn finish_behavior_docking_actions_after_matching_arrival(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut occupancy: ResMut<super::interaction_container_occupancy::InteractionContainerOccupancy>,
    mut behavior_tasks: Query<
        (
            &mut BehaviorTaskExecutionState,
            &PendingBehaviorDockingCompletion,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for arrival in arrivals.read() {
        let Ok((mut behavior_task, pending_docking)) = behavior_tasks.get_mut(arrival.entity)
        else {
            continue;
        };
        if arrival.target != Some(pending_docking.target)
            || arrival.request_id != pending_docking.request_id
            || !pending_docking.still_owns_exact_task_frame(&behavior_task)
            || !behavior_document_assets
                .get(&behavior_task.document)
                .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
                .is_some_and(|action| matches!(action, BehaviorAction::Dock(_)))
        {
            continue;
        }
        // Immediate arrival is valid: next_action_tick gates starting actions.
        // Advancing synchronously rejects duplicates before removal flushes.
        occupancy.mark_docked(pending_docking.target, arrival.entity);
        advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
        commands
            .entity(arrival.entity)
            .remove::<PendingBehaviorDockingCompletion>();
    }
}

pub(super) fn fail_behavior_docking_actions_after_navigation_failure(
    mut commands: Commands,
    mut failures: MessageReader<NavigationFailed>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    waiting_tasks: Query<
        (
            &BehaviorTaskExecutionState,
            &PendingBehaviorDockingCompletion,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for failure in failures.read() {
        let waiting_for_dock = waiting_tasks
            .get(failure.entity)
            .ok()
            .filter(|(task, pending)| {
                failure.request_id == pending.request_id
                    && pending.still_owns_exact_task_frame(task)
            })
            .and_then(|(task, _)| {
                documents
                    .get(&task.document)
                    .and_then(|document| find_current_behavior_task_action(document, task))
            })
            .is_some_and(|action| matches!(action, BehaviorAction::Dock(_)));
        if waiting_for_dock {
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                failure.entity, &mut commands,
            );
        }
    }
}
