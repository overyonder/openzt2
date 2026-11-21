use super::*;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationRigJoints;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorSpawnObject, RunningBehaviorInstruction,
};

pub(super) fn deliver_behavior_object_spawns(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    clock: Res<ZooClock>,
    mut ids: Query<&mut PersistentIdAllocator>,
    mut events: MessageReader<AnimationObjectCommand>,
    mut completed: MessageReader<AnimationCompleted>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    owners: Query<(&AnimationPresentationOwner, &AnimationPlaybackController)>,
    rigs: Query<&AnimationRigJoints>,
    joints: Query<(&AnimationJointTarget, &GlobalTransform)>,
    children: Query<&Children>,
    named_nodes: super::super::behavior_docking_execution::NamedNodeTransforms,
    pose_validation: super::spawn_pose::SpawnPoseValidation,
    mut actors: Query<
        (
            (&BehaviorTaskExecutionState, &RunningBehaviorInstruction, &RunBehaviorSpawnObject),
            &mut PendingBehaviorObjectSpawn,
            &WorldMember,
            &mut openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let mut failed = bevy::platform::collections::HashSet::new();
    for rejection in rejected.read() {
        let actor = rejection.animation_subject_entity;
        if actors
            .get(actor)
            .is_ok_and(|((task, instruction, _), pending, _, _)| {
                instruction.is_current(task)
                    && pending.owns(task, instruction)
                    && pending.request == Some(rejection.request_id)
            })
        {
            failed.insert(actor);
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
    for event in events.read() {
        if event.action != AuthoredAnimationTextAction::SpawnObject {
            continue;
        }
        let Ok((owner, playback)) = owners.get(event.controller) else {
            continue;
        };
        let actor = owner.gameplay_entity;
        let Ok(((task, instruction, action), mut pending, world, mut random)) =
            actors.get_mut(actor)
        else {
            continue;
        };
        if failed.contains(&actor)
            || pending.spawned
            || !instruction.is_current(task)
            || !pending.owns(task, instruction)
            || pending.request.is_none()
            || pending.request != event.request_id
            || playback.explicit_clip_request_id != event.request_id
            || playback.playback_generation != event.playback_generation
        {
            continue;
        }
        let RunBehaviorSpawnObject {
            entity,
            subject_node,
            pose,
            ..
        } = action;
        let spawn = (|| {
            let view = definitions.get(&definition_assets)?;
            let object = view.find_object(*entity)?;
            // The spawn node, such as an animal's `Poop`, may be an animated
            // joint or a plain named node. Without either the spawn fails.
            let subject_joint = AssetId::from_key(subject_node);
            let transform = rigs.get(event.controller).ok()
                .into_iter().flat_map(|rig| joints.iter_many(rig.joints()).matched())
                .find(|(joint, _)| joint.authored_joint_id == subject_joint)
                .map(|(_, transform)| transform.compute_transform())
                .or_else(|| {
                    super::super::behavior_docking_execution::find_named_descendant_global_transform(
                        actor,
                        subject_node,
                        &children,
                        &named_nodes,
                    )
                    .map(GlobalTransform::compute_transform)
                })?;
            let presentation = if object.prefab == AssetId::default() {
                None
            } else {
                let handle = pending.prefab.as_ref()?;
                Some((handle, prefabs.get(handle)?))
            };
            let id = ids.get_mut(world.root).ok()?.allocate().ok()?;
            let placement = pose_validation.select_spawn_pose(
                actor,
                *world,
                object.id,
                transform,
                pose_validation.actor_pose(actor)?,
                *pose,
                view,
                &mut random.0,
            );
            let transform = placement
                .transform
                .mul_transform(Transform::from_scale(Vec3::splat(object.prefab_scale)));
            let spawned = if let Some((handle, prefab)) = presentation {
                spawn_loaded_scene_prefab_as_world_instance(
                    &mut commands,
                    prefab,
                    handle.clone(),
                    world.root,
                    object.id,
                    id,
                    transform,
                    true,
                    None,
                    RigidBody::Kinematic,
                )
            } else {
                commands
                    .spawn((
                        *world,
                        DefinitionId(object.id),
                        id,
                        transform,
                        Visibility::Inherited,
                        Inspectable {
                            definition: object.id,
                        },
                    ))
                    .id()
            };
            placement.apply_stomps(&mut commands);
            if object
                .properties
                .contains_all(WorldObjectPropertyFlags::ANIMAL_WASTE)
            {
                // BFBehSpawn creates one discrete object, not a timed quantity.
                commands.entity(spawned).insert(AnimalHabitatWaste {
                    uncontained_waste_units: 1,
                });
            }
            Some(())
        })();
        if spawn.is_some() {
            pending.spawned = true;
        } else {
            failed.insert(actor);
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
    for event in completed.read() {
        let Ok((owner, _)) = owners.get(event.animation_playback_controller_entity) else {
            continue;
        };
        let actor = owner.gameplay_entity;
        let Ok(((task, instruction, _), pending, _, _)) = actors.get_mut(actor) else {
            continue;
        };
        if failed.contains(&actor)
            || !instruction.is_current(task)
            || !pending.owns(task, instruction)
            || pending.request.is_none()
            || pending.request != event.explicit_clip_request_id
        {
            continue;
        }
        if pending.spawned {
            instruction.complete(actor, clock.tick, &mut commands);
        } else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
        commands
            .entity(actor)
            .remove::<PendingBehaviorObjectSpawn>();
    }
}
