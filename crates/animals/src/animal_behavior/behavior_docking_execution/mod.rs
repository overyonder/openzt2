use super::behavior_container_admission::BehaviorAdmissionInstruction;
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::behavior::action::docking::BehaviorDockAction;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition;
use openzt2_zoo_state::animal_behavior::behavior_target_placement_types::{
    travel_target, BehaviorPlacementTravelSite,
};
use openzt2_zoo_state::animal_behavior::interaction_container_membership::requests::{
    InteractionContainerAdmissionOutcome, InteractionContainerAdmissionRequest,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;

#[cfg(test)]
mod navigation_event_tests;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorDock, RunBehaviorDockQueue, RunningBehaviorInstruction,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::{
    DockedAtInteractionContainer, InteractionContainerMemberships,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::DockAt;
use openzt2_zoo_state::locomotion::locomotion_types::{NavAgent, NavigationRequestGeneration};
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectAuthoredEntrance;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

pub(super) type DockBehaviorActors<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static NavAgent>,
        Option<&'static LocomotionMode>,
        &'static mut BehaviorTaskExecutionState,
        &'static RunningBehaviorInstruction,
        Option<&'static RunBehaviorDock>,
        Option<&'static RunBehaviorDockQueue>,
    ),
    (
        Without<PendingBehaviorDockingCompletion>,
        Without<PendingBehaviorTaskFailure>,
        Or<(With<RunBehaviorDock>, With<RunBehaviorDockQueue>)>,
    ),
>;

#[derive(SystemParam)]
pub(super) struct DockBehaviorGeometry<'w, 's> {
    target_entrances: Query<'w, 's, &'static PlacedObjectAuthoredEntrance>,
    fences: Query<'w, 's, openzt2_zoo_state::topology::topology_graph_types::FenceSegment>,
    topology_nodes:
        Query<'w, 's, &'static openzt2_zoo_state::topology::topology_graph_types::TopologyNode>,
    topology_grids:
        Query<'w, 's, &'static openzt2_zoo_state::topology::topology_graph_types::TopologyGrid>,
    world_members: Query<
        'w,
        's,
        &'static openzt2_world_state::world_spawn::world_membership_types::WorldMember,
    >,
    ground: openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility<'w, 's>,
    children: Query<'w, 's, &'static Children>,
    authored_attachment_transforms: NamedNodeTransforms<'w, 's>,
    entity_global_transforms: Query<'w, 's, &'static GlobalTransform>,
    memberships: InteractionContainerMemberships<'w, 's>,
}

pub(super) fn start_current_supported_behavior_docking_actions(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    simulation_clock: Single<&ZooClock>,
    mut behavior_tasks: DockBehaviorActors,
    target_definition_references: Query<&DefinitionId>,
    travel_sites: Query<&BehaviorPlacementTravelSite>,
    path_tiles: Query<(), With<openzt2_zoo_state::topology::topology_graph_types::PathTile>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    geometry: DockBehaviorGeometry,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut docking_requests: MessageWriter<DockAt>,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let DockBehaviorGeometry {
        fences,
        children,
        authored_attachment_transforms,
        entity_global_transforms,
        memberships,
        ..
    } = &geometry;
    for (
        actor,
        navigation_agent,
        locomotion_mode,
        mut behavior_task,
        instruction,
        dock,
        queue_approach,
    ) in behavior_action_actors(&mut behavior_tasks, activation.0)
    {
        if activation.0.is_none() && !instruction.can_update_after_activation(simulation_clock.tick)
        {
            continue;
        }
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        if !instruction.is_current(&behavior_task) {
            continue;
        }
        let docking_action = dock.map(|dock| &dock.0);
        let queue_approach = queue_approach.map(|queue| &queue.0);
        // Agents can be added later, as when an animal's walk clip finishes loading.
        let Some(navigation_agent) = navigation_agent else {
            debug!(?actor, program = ?behavior_task.program,
                "dock waits: the actor has no navigation agent");
            continue;
        };
        let Some(target) = travel_target(&behavior_task, travel_sites.get(actor).ok()) else {
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
            let tag = instruction.reservation_tag();
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
                    children,
                    authored_attachment_transforms,
                ) else {
                    continue;
                };
                node.translation()
            };
            let offset = (actor_transform.translation() - queue_node_position).xz();
            if offset.length() <= queue_approach.hit_radius_m {
                instruction.complete(actor, simulation_clock.tick, &mut commands);
                continue;
            }
            let direction = offset.normalize_or_zero();
            let direction = Vec3::new(direction.x, 0.0, direction.y);
            let target_from_world = target_transform.affine().inverse();
            let Ok(mut generation) = navigation_generations.get_mut(actor) else {
                continue;
            };
            let request_id = generation.advance();
            docking_requests.write(DockAt {
                entity: actor,
                request_id,
                target,
                local_point: target_from_world.transform_point3(
                    queue_node_position + direction * queue_approach.hit_radius_m,
                ),
                local_forward: target_from_world.transform_vector3(-direction),
                radius_m: navigation_agent.radius_m.max(0.05),
            });
            commands
                .entity(actor)
                .insert(PendingBehaviorDockingCompletion {
                    target,
                    instruction: instruction.id(),
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
        if let Some(reservation_tag) = docking_action
            .reservation
            .or(instruction.reservation_tag())
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
            let slots = memberships.service_slot_candidates(
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
                            children,
                            authored_attachment_transforms,
                        )?
                        .translation()
                    };
                    Some((position - actor_transform.translation()).xz())
                },
            );
            let request = InteractionContainerAdmissionRequest::Admit {
                container: target,
                member: actor,
                slots,
                despawn_on_refusal: false,
                queue_service_slots: Vec::new(),
            };
            let action = docking_action.clone();
            let instruction = BehaviorAdmissionInstruction::new(actor, &behavior_task, instruction);
            commands.queue(move |world: &mut World| {
                if instruction.is_current(world) {
                    let outcome = request.commit(world);
                    world.trigger(BehaviorDockAdmitted {
                        actor,
                        target,
                        action,
                        outcome,
                    });
                }
            });
            continue;
        }
        request_behavior_dock(
            actor,
            target,
            docking_action,
            target_definition,
            navigation_agent,
            locomotion_mode,
            simulation_clock.tick,
            &mut behavior_task,
            instruction,
            &geometry,
            &mut navigation_generations,
            &mut docking_requests,
            &mut commands,
        );
    }
}

#[derive(EntityEvent)]
pub(super) struct BehaviorDockAdmitted {
    #[event_target]
    actor: Entity,
    target: Entity,
    action: BehaviorDockAction,
    outcome: InteractionContainerAdmissionOutcome,
}

pub(super) fn request_dock_after_container_admission(
    admitted: On<BehaviorDockAdmitted>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    mut actors: DockBehaviorActors,
    target_definitions: Query<&DefinitionId>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    geometry: DockBehaviorGeometry,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut requests: MessageWriter<DockAt>,
) {
    let actor = admitted.actor;
    let target = admitted.target;
    let Ok((_, Some(agent), mode, mut task, instruction, _, _)) = actors.get_mut(actor) else {
        return;
    };
    let InteractionContainerAdmissionOutcome::Granted { slot, .. } = admitted.outcome else {
        super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
            actor,
            &mut commands,
        );
        return;
    };
    let definition = definitions
        .get(&definition_assets)
        .and_then(|definitions| definitions.find_object(target_definitions.get(target).ok()?.0));
    if !instruction.is_current(&task) {
        return;
    }
    task.interaction_slot = Some(slot);
    request_behavior_dock(
        actor,
        target,
        &admitted.action,
        definition,
        agent,
        mode,
        clock.tick,
        &mut task,
        instruction,
        &geometry,
        &mut navigation_generations,
        &mut requests,
        &mut commands,
    );
}

#[allow(clippy::too_many_arguments)]
fn request_behavior_dock(
    actor: Entity,
    target: Entity,
    docking_action: &BehaviorDockAction,
    target_definition: Option<&WorldObjectDefinition>,
    navigation_agent: &NavAgent,
    locomotion_mode: Option<&LocomotionMode>,
    tick: u64,
    behavior_task: &mut BehaviorTaskExecutionState,
    instruction: &RunningBehaviorInstruction,
    geometry: &DockBehaviorGeometry,
    navigation_generations: &mut Query<&mut NavigationRequestGeneration>,
    docking_requests: &mut MessageWriter<DockAt>,
    commands: &mut Commands,
) {
    let DockBehaviorGeometry {
        target_entrances,
        fences,
        topology_nodes,
        topology_grids,
        world_members,
        ground,
        children,
        authored_attachment_transforms,
        entity_global_transforms,
        memberships,
    } = geometry;
    if !docking_action.redock && memberships.is_docked(target, actor) {
        instruction.complete(actor, tick, commands);
        return;
    }
    let (mut local_point, local_forward) = if let Some(radial_distance_m) =
        docking_action.radial_distance_m
    {
        let (Ok(actor_transform), Ok(target_transform)) = (
            entity_global_transforms.get(actor),
            entity_global_transforms.get(target),
        ) else {
            return;
        };
        // A fence segment's origin is its first end; dock on its middle,
        // on the ground there rather than at the topology layer's height.
        let centre = fences.get(target).ok().and_then(|edge| {
            let root = world_members.get(actor).ok()?.root;
            let topology_grid = topology_grids.get(root).ok()?;
            let first = topology_grid.cell_translation(topology_nodes.get(edge.a()).ok()?.cell);
            let second = topology_grid.cell_translation(topology_nodes.get(edge.b()).ok()?.cell);
            let middle = (first + second) * 0.5;
            let middle = middle.with_y(ground.ground_height(root, middle.xz()).unwrap_or(middle.y));
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
                memberships
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
                children,
                authored_attachment_transforms,
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
            return;
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
            children,
            authored_attachment_transforms,
        ) else {
            // Radial subject nodes are often authored position attributes
            // rather than model or virtual nodes; waiting cannot supply one.
            if docking_action.radial_distance_m.is_some() {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(
                    actor, commands,
                );
            } else {
                debug!(
                    ?actor,
                    subject_node = subject_node_name,
                    "dock waits: the actor's subject node is absent"
                );
            }
            return;
        };
        let Some(actor_global_transform) = entity_global_transforms.get(actor).ok() else {
            return;
        };
        let subject_node_local_position = actor_global_transform
            .affine()
            .inverse()
            .transform_point3(subject_node_global_transform.translation());
        let Ok(target_global_transform) = entity_global_transforms.get(target) else {
            return;
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
    commands
        .entity(actor)
        .try_remove::<DockedAtInteractionContainer>();
    let Ok(mut generation) = navigation_generations.get_mut(actor) else {
        return;
    };
    let request_id = generation.advance();
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
            instruction: instruction.id(),
            request_id,
        });
    behavior_task.next_action_tick = tick.saturating_add(1);
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
    simulation_clock: Single<&ZooClock>,
    memberships: InteractionContainerMemberships,
    travel_sites: Query<&BehaviorPlacementTravelSite>,
    behavior_tasks: Query<
        (
            &BehaviorTaskExecutionState,
            &PendingBehaviorDockingCompletion,
            &RunningBehaviorInstruction,
            Has<RunBehaviorDock>,
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Or<(With<RunBehaviorDock>, With<RunBehaviorDockQueue>)>,
        ),
    >,
) {
    for arrival in arrivals.read() {
        let Ok((behavior_task, pending_docking, instruction, dock)) =
            behavior_tasks.get(arrival.entity)
        else {
            continue;
        };
        debug!(actor = ?arrival.entity, request_id = arrival.request_id,
            expected_request_id = pending_docking.request_id, "dock arrival received");
        if arrival.target != Some(pending_docking.target)
            || arrival.request_id != pending_docking.request_id
            || !pending_docking.owns(
                behavior_task,
                instruction,
                travel_target(behavior_task, travel_sites.get(arrival.entity).ok()),
            )
        {
            continue;
        }
        // A queue approach leaves the actor undocked.
        if dock
            && memberships
                .actor_slot(pending_docking.target, arrival.entity)
                .is_some()
        {
            commands
                .entity(arrival.entity)
                .try_insert(DockedAtInteractionContainer);
        }
        commands
            .entity(arrival.entity)
            .remove::<PendingBehaviorDockingCompletion>();
        instruction.complete(arrival.entity, simulation_clock.tick, &mut commands);
    }
}

pub(super) fn fail_behavior_docking_actions_after_navigation_failure(
    mut commands: Commands,
    mut failures: MessageReader<NavigationFailed>,
    travel_sites: Query<&BehaviorPlacementTravelSite>,
    waiting_tasks: Query<
        (
            &BehaviorTaskExecutionState,
            &PendingBehaviorDockingCompletion,
            &RunningBehaviorInstruction,
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Or<(With<RunBehaviorDock>, With<RunBehaviorDockQueue>)>,
        ),
    >,
) {
    for failure in failures.read() {
        let waiting_for_dock =
            waiting_tasks
                .get(failure.entity)
                .is_ok_and(|(task, pending, instruction)| {
                    failure.request_id == pending.request_id
                        && pending.owns(
                            task,
                            instruction,
                            travel_target(task, travel_sites.get(failure.entity).ok()),
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
