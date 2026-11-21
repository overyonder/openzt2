use bevy::prelude::*;

#[cfg(test)]
mod navigation_event_tests;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::DockAt;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectAuthoredEntrance;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

/// How far apart guests stand in a queue: a little more than two people's
/// spacing, so the line does not push itself apart.
const QUEUE_PLACE_SPACING_M: f32 = 0.8;

/// Where the guest `place` places back in a queue stands, and the direction
/// the line runs. The line starts at the queue node and runs out away from
/// the stand, each guest one place behind whoever joined before them, as far
/// as the authored queue reaches. The authored hit radius is how near counts
/// as there, not how far away to stand.
fn queue_place(
    queue_node: Vec3,
    stand: Vec3,
    actor: Vec3,
    place: usize,
    queue_radius_m: f32,
) -> (Vec3, Vec3) {
    let line = (queue_node - stand).xz();
    let line = if line.length_squared() > 0.01 {
        line
    } else {
        (actor - queue_node).xz()
    }
    .normalize_or_zero();
    let direction = Vec3::new(line.x, 0.0, line.y);
    let distance = (place as f32 * QUEUE_PLACE_SPACING_M).min(queue_radius_m);
    (queue_node + direction * distance, direction)
}

pub(super) fn start_current_supported_behavior_docking_actions(
    mut commands: Commands,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<
        (
            Entity,
            Option<&NavAgent>,
            Option<&LocomotionMode>,
            &mut BehaviorTaskExecutionState,
        ),
        (
            Without<PendingBehaviorDockingCompletion>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    target_entrances: Query<&PlacedObjectAuthoredEntrance>,
    target_definition_references: Query<&DefinitionId>,
    path_tiles: Query<(), With<openzt2_zoo_state::topology::topology_graph_types::PathTile>>,
    (fences, topology_nodes, topology_grid, ground): (
        Query<&openzt2_zoo_state::topology::topology_graph_types::FenceEdge>,
        Query<&openzt2_zoo_state::topology::topology_graph_types::TopologyNode>,
        Res<openzt2_zoo_state::topology::topology_graph_types::TopologyGrid>,
        openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility,
    ),
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    children: Query<&Children>,
    authored_attachment_transforms: NamedNodeTransforms,
    entity_global_transforms: Query<&GlobalTransform>,
    mut navigation_request_sequence: ResMut<
        openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestSequence,
    >,
    mut docking_requests: MessageWriter<DockAt>,
    (mut occupancy, walking_to_their_place): (
        ResMut<super::interaction_container_occupancy::InteractionContainerOccupancy>,
        Query<(Entity, &BehaviorTaskExecutionState), With<PendingBehaviorDockingCompletion>>,
    ),
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    // Everyone in a queue, standing in it or still walking to their place:
    // running a set that walks up to it.
    let waiting_in_queues = behavior_tasks
        .iter()
        .map(|(actor, .., task)| (actor, task))
        .chain(walking_to_their_place.iter())
        .filter(|(_, task)| {
            behavior_document_assets
                .get(&task.document)
                .and_then(|document| document.behavior_set_at_index(task.declaration))
                .is_some_and(|set| {
                    set.actions
                        .lowered_actions()
                        .iter()
                        .any(|action| matches!(action, BehaviorAction::DockQueue(_)))
                })
        })
        .filter_map(|(actor, task)| Some((actor, task.target()?, task.execution_id)))
        .collect::<Vec<_>>();
    for (actor, navigation_agent, locomotion_mode, mut behavior_task) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let (docking_action, queue_approach) = match behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        {
            Some(BehaviorAction::Dock(docking_action)) => (Some(docking_action), None),
            Some(BehaviorAction::DockQueue(queue_approach)) => (None, Some(queue_approach)),
            _ => continue,
        };
        // Agents can be added later, as when an animal's walk clip finishes loading.
        let Some(navigation_agent) = navigation_agent else {
            debug!(?actor, program = ?behavior_task.program,
                "dock waits: the actor has no navigation agent");
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
        // Joining a queue takes no service place and leaves the actor undocked,
        // so the service dock that follows still walks to its reserved slot.
        if let Some(queue_approach) = queue_approach {
            let tag = behavior_task.reservation_tag(&behavior_document_assets);
            let Some(queue_node_name) = target_definition.and_then(|definition| {
                definition
                    .interaction_slots
                    .iter()
                    .find(|slot| slot.is_queue && tag.is_none_or(|tag| slot.reservation_tag == tag))
                    .map(|slot| slot.target_node_name.as_str())
            }) else {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                    actor, &mut commands,
                );
                continue;
            };
            let (Ok(actor_transform), Ok(target_transform)) = (
                entity_global_transforms.get(actor),
                entity_global_transforms.get(target),
            ) else {
                continue;
            };
            let queue_node_position = if queue_node_name.is_empty() {
                target_transform.translation()
            } else {
                let Some(node) = find_named_descendant_global_transform(
                    target,
                    queue_node_name,
                    &children,
                    &authored_attachment_transforms,
                ) else {
                    continue;
                };
                node.translation()
            };
            let place = waiting_in_queues
                .iter()
                .filter(|(other, other_target, joined)| {
                    *other != actor
                        && *other_target == target
                        && *joined < behavior_task.execution_id
                })
                .count();
            let (spot, direction) = queue_place(
                queue_node_position,
                target_transform.translation(),
                actor_transform.translation(),
                place,
                queue_approach.queue_radius_m,
            );
            if (actor_transform.translation() - spot).xz().length() <= QUEUE_PLACE_SPACING_M * 0.5 {
                advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
                continue;
            }
            let target_from_world = target_transform.affine().inverse();
            let request_id = navigation_request_sequence.next();
            docking_requests.write(DockAt {
                entity: actor,
                request_id,
                target,
                local_point: target_from_world.transform_point3(spot),
                local_forward: target_from_world.transform_vector3(-direction),
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
            continue;
        }
        let docking_action = docking_action.expect("a dock or a queue approach");
        // A path holds any number of guests, and a fence any number of
        // animals; their reservation tags name no slot. Nor does a tag such as
        // `General` on a target without a slot of that tag, like the guest a
        // rampaging ankylosaur attacks.
        if let Some(reservation_tag) = behavior_task
            .reservation_tag(&behavior_document_assets)
            .filter(|_| !path_tiles.contains(target) && !fences.contains(target))
            .filter(|tag| {
                target_definition.is_none_or(|definition| {
                    definition
                        .interaction_slots
                        .iter()
                        .any(|slot| slot.reservation_tag == *tag)
                })
            })
        {
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
        let (mut local_point, local_forward) = if let Some(radial_distance_m) =
            docking_action.radial_distance_m
        {
            let (Ok(actor_transform), Ok(target_transform)) = (
                entity_global_transforms.get(actor),
                entity_global_transforms.get(target),
            ) else {
                continue;
            };
            // A fence segment's origin is its first end; dock on its middle,
            // on the ground there rather than at the topology layer's height.
            let centre = fences.get(target).ok().and_then(|edge| {
                let first = topology_grid.cell_translation(topology_nodes.get(edge.a).ok()?.cell);
                let second = topology_grid.cell_translation(topology_nodes.get(edge.b).ok()?.cell);
                let middle = (first + second) * 0.5;
                let middle = middle.with_y(ground.ground_height(middle.xz()).unwrap_or(middle.y));
                Some(target_transform.affine().inverse().transform_point3(middle))
            });
            radial_dock_point_and_forward_in_target_space(
                actor_transform,
                target_transform,
                centre.unwrap_or(Vec3::ZERO),
                radial_distance_m,
                docking_action.rotation_tolerance_radians,
            )
        } else {
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
                debug!(?actor, ?target, target_node = ?target_node_name,
                        "dock waits: the target node is absent");
                continue;
            }
            authored_named_target_dock.unwrap_or_else(|| {
                target_entrances
                    .get(target)
                    .map_or((Vec3::ZERO, Vec3::Z), |entrance| {
                        (entrance.local_position, entrance.local_forward)
                    })
            })
        };
        if let Some(subject_node_name) = docking_action.subject_node_name.as_deref() {
            let Some(subject_node_global_transform) = find_named_descendant_global_transform(
                actor,
                subject_node_name,
                &children,
                &authored_attachment_transforms,
            ) else {
                // Radial subject nodes are often authored position attributes
                // rather than model or virtual nodes; waiting cannot supply one.
                if docking_action.radial_distance_m.is_some() {
                    super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                        actor, &mut commands,
                    );
                } else {
                    debug!(
                        ?actor,
                        subject_node = subject_node_name,
                        "dock waits: the actor's subject node is absent"
                    );
                }
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
                locomotion_mode.copied().unwrap_or_default(),
            );
        }
        occupancy.begin_redocking(actor);
        let request_id = navigation_request_sequence.next();
        debug!(?actor, ?target, request_id, ?local_point, slot = ?behavior_task.interaction_slot,
            "dock requested");
        docking_requests.write(DockAt {
            entity: actor,
            request_id,
            target,
            local_point,
            local_forward,
            radius_m: docking_action
                .arrival_radius_m
                .unwrap_or(navigation_agent.radius_m.max(0.05)),
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

fn radial_dock_point_and_forward_in_target_space(
    actor_transform: &GlobalTransform,
    target_transform: &GlobalTransform,
    target_centre: Vec3,
    radial_distance_m: f32,
    rotation_tolerance_radians: f32,
) -> (Vec3, Vec3) {
    let target_affine = target_transform.affine();
    let target_world_position = target_affine.transform_point3(target_centre);
    // An actor already on the target origin approaches from the target's front.
    let target_world_forward = target_affine.transform_vector3(Vec3::Z);
    let approach_direction = (actor_transform.translation() - target_world_position)
        .xz()
        .try_normalize()
        .or_else(|| target_world_forward.xz().try_normalize())
        .unwrap_or(Vec2::Y);
    let approach_direction = Vec3::new(approach_direction.x, 0.0, approach_direction.y);
    let actor_forward = actor_transform
        .affine()
        .transform_vector3(Vec3::Z)
        .xz()
        .normalize_or_zero();
    let facing = -approach_direction;
    let heading_error = actor_forward.dot(facing.xz()).clamp(-1.0, 1.0).acos();
    let forward = if actor_forward != Vec2::ZERO && heading_error <= rotation_tolerance_radians {
        Vec3::new(actor_forward.x, 0.0, actor_forward.y)
    } else {
        facing
    };
    let target_from_world = target_affine.inverse();
    (
        target_from_world
            .transform_point3(target_world_position + approach_direction * radial_distance_m),
        target_from_world.transform_vector3(forward),
    )
}

fn align_subject_attachment_with_target_dock(
    local_point: Vec3,
    local_forward: Vec3,
    subject_node_local_position: Vec3,
    actor_transform: &GlobalTransform,
    target_transform: &GlobalTransform,
    locomotion_mode: LocomotionMode,
) -> Vec3 {
    // Resolve the scaled attachment in the final world orientation before
    // converting its offset through the target's independent transform.
    let target_affine = target_transform.affine();
    let world_forward = target_affine.transform_vector3(local_forward);
    let horizontal_forward = Vec2::new(world_forward.x, world_forward.z).normalize_or_zero();
    let rotation = if horizontal_forward == Vec2::ZERO {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_y(horizontal_forward.x.atan2(horizontal_forward.y))
    };
    let (actor_scale, _, _) = actor_transform.to_scale_rotation_translation();
    let mut world_offset = rotation * (actor_scale * subject_node_local_position);
    // Ground actors use the attachment's horizontal reach without lowering
    // their root beneath the dock. Swimming and flight retain 3D alignment.
    if matches!(
        locomotion_mode,
        LocomotionMode::Ground | LocomotionMode::Vehicle
    ) {
        world_offset.y = 0.0;
    }
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
        let mouth = Vec3::new(0.0, 1.45, 2.0);
        for mode in [
            LocomotionMode::Ground,
            LocomotionMode::Vehicle,
            LocomotionMode::Swim,
            LocomotionMode::Flight,
        ] {
            let point = align_subject_attachment_with_target_dock(
                socket,
                Vec3::Z,
                mouth,
                &actor,
                &target,
                mode,
            );
            let root = target.transform_point(point);
            let dock = target.transform_point(socket);
            let subject_attachment_world = root + Vec3::new(1.0, 0.725, 0.0);
            assert!(subject_attachment_world.xz().abs_diff_eq(dock.xz(), 1.0e-5));
            if matches!(mode, LocomotionMode::Ground | LocomotionMode::Vehicle) {
                assert!((root.y - dock.y).abs() < 1.0e-5);
            } else {
                assert!(subject_attachment_world.abs_diff_eq(dock, 1.0e-5));
            }
        }
    }

    #[test]
    fn a_fence_dock_stops_short_of_the_segment_middle_not_its_first_end() {
        // A 4 m segment from (0, 0, 0) along +X; the animal stands to its north.
        let segment = GlobalTransform::IDENTITY;
        let (point, _) = radial_dock_point_and_forward_in_target_space(
            &GlobalTransform::from_translation(Vec3::new(2.0, 0.0, -6.0)),
            &segment,
            Vec3::new(2.0, 0.0, 0.0),
            2.0,
            0.0,
        );
        assert!(point.abs_diff_eq(Vec3::new(2.0, 0.0, -2.0), 1.0e-5));
    }

    #[test]
    fn radial_dock_stops_at_the_authored_distance_on_the_actor_side_facing_the_target() {
        let target = GlobalTransform::from(Transform {
            translation: Vec3::new(10.0, 2.0, 20.0),
            rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            scale: Vec3::splat(3.0),
        });
        let (point, forward) = radial_dock_point_and_forward_in_target_space(
            &GlobalTransform::from_translation(Vec3::new(10.0, 0.0, 30.0)),
            &target,
            Vec3::ZERO,
            1.5,
            0.0,
        );
        assert!(target
            .transform_point(point)
            .abs_diff_eq(Vec3::new(10.0, 2.0, 21.5), 1.0e-5));
        assert!(target
            .affine()
            .transform_vector3(forward)
            .normalize()
            .abs_diff_eq(Vec3::NEG_Z, 1.0e-5));
    }

    #[test]
    fn radial_shelter_dock_accepts_an_opposite_heading() {
        let actor = GlobalTransform::from_translation(Vec3::new(0.0, 0.0, 10.0));
        let (point, forward) = radial_dock_point_and_forward_in_target_space(
            &actor,
            &GlobalTransform::IDENTITY,
            Vec3::ZERO,
            3.0,
            std::f32::consts::PI,
        );
        assert!(point.abs_diff_eq(Vec3::new(0.0, 0.0, 3.0), 1.0e-5));
        assert!(forward.abs_diff_eq(Vec3::Z, 1.0e-5));
    }
}

/// Named points behaviors aim at: prefab attachment points and virtual nodes,
/// and the authored nodes of animated models, such as an animal's `Node_Mouth`.
pub(super) type NamedNodeTransforms<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static PrefabAuthoredAttachmentIdentifier>,
        Option<&'static Name>,
        &'static GlobalTransform,
    ),
    Or<(With<PrefabAuthoredAttachmentIdentifier>, With<Name>)>,
>;

pub(super) fn find_named_descendant_global_transform<'a>(
    root: Entity,
    requested_name: &str,
    children: &Query<&Children>,
    named_nodes: &'a NamedNodeTransforms,
) -> Option<&'a GlobalTransform> {
    let requested_name = requested_name.trim();
    let requested_attachment_identifier =
        openzt2_game_data::AssetId::from_key(&requested_name.to_ascii_lowercase());
    children
        .iter_descendants_depth_first::<Children>(root)
        .find_map(|descendant| {
            let (identifier, name, transform) = named_nodes.get(descendant).ok()?;
            (identifier.is_some_and(|identifier| identifier.0 == requested_attachment_identifier)
                || name.is_some_and(|name| name.as_str().eq_ignore_ascii_case(requested_name)))
            .then_some(transform)
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
        debug!(actor = ?arrival.entity, request_id = arrival.request_id,
            expected_request_id = pending_docking.request_id, "dock arrival received");
        if arrival.target != Some(pending_docking.target)
            || arrival.request_id != pending_docking.request_id
            || !pending_docking.still_owns_exact_task_frame(&behavior_task)
            || !behavior_document_assets
                .get(&behavior_task.document)
                .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
                .is_some_and(|action| {
                    matches!(
                        action,
                        BehaviorAction::Dock(_) | BehaviorAction::DockQueue(_)
                    )
                })
        {
            continue;
        }
        // Immediate arrival is valid: next_action_tick gates starting actions.
        // Advancing synchronously rejects duplicates before removal flushes.
        // A queue approach leaves the actor undocked.
        if behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
            .is_some_and(|action| matches!(action, BehaviorAction::Dock(_)))
        {
            occupancy.mark_docked(pending_docking.target, arrival.entity);
        }
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
            .is_some_and(|action| {
                matches!(
                    action,
                    BehaviorAction::Dock(_) | BehaviorAction::DockQueue(_)
                )
            });
        if waiting_for_dock {
            debug!(actor = ?failure.entity, reason = ?failure.reason,
                "behavior docking failed to navigate");
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                failure.entity, &mut commands,
            );
        }
    }
}

#[cfg(test)]
mod queue_place_tests {
    use super::*;

    #[test]
    fn a_queue_lines_up_behind_its_node_up_to_its_reach() {
        let node = Vec3::new(2.0, 0.0, 0.0);
        let place = |n| queue_place(node, Vec3::ZERO, Vec3::new(0.0, 0.0, 9.0), n, 12.0).0;
        assert_eq!(place(0), node);
        assert_eq!(place(1), Vec3::new(2.0 + QUEUE_PLACE_SPACING_M, 0.0, 0.0));
        assert_eq!(place(100), Vec3::new(14.0, 0.0, 0.0));
    }
}
