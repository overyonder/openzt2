use super::*;

pub(super) fn deliver_behavior_object_spawns(
    mut commands: Commands,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    clock: Res<ZooClock>,
    mut ids: ResMut<PersistentIdAllocator>,
    mut events: MessageReader<AnimationObjectCommand>,
    mut completed: MessageReader<AnimationCompleted>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    owners: Query<(&AnimationPresentationOwner, &AnimationPlaybackController)>,
    joints: Query<(&AnimationJointTarget, &GlobalTransform)>,
    children: Query<&Children>,
    named_nodes: super::super::behavior_docking_execution::NamedNodeTransforms,
    actor_transforms: Query<&GlobalTransform, With<BehaviorTaskExecutionState>>,
    mut actors: Query<
        (
            &mut BehaviorTaskExecutionState,
            &mut PendingBehaviorObjectSpawn,
            &WorldMember,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let mut failed = bevy::platform::collections::HashSet::new();
    for rejection in rejected.read() {
        let actor = rejection.animation_subject_entity;
        if actors.get(actor).is_ok_and(|(task, pending, _)| {
            pending.owns(&task) && pending.request == Some(rejection.request_id)
        }) {
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
        let Ok((task, mut pending, world)) = actors.get_mut(actor) else {
            continue;
        };
        if failed.contains(&actor)
            || pending.spawned
            || !pending.owns(&task)
            || pending.request.is_none()
            || pending.request != event.request_id
            || playback.explicit_clip_request_id != event.request_id
            || playback.playback_generation != event.playback_generation
        {
            continue;
        }
        let Some(BehaviorAction::SpawnObject {
            entity,
            subject_node,
            ..
        }) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let spawn = (|| {
            let view = definitions.get(&definition_assets)?;
            let object = view.find_object(*entity)?;
            // Many models carry the spawn node (an animal's `Poop`) as a plain
            // named node rather than an animated joint, and some lack it; the
            // object then drops at the actor's position.
            let transform = joints
                .iter()
                .find(|(joint, _)| {
                    joint.animation_playback_controller_entity == event.controller
                        && joint.joint_asset_key == *subject_node
                })
                .map(|(_, transform)| transform.compute_transform())
                .or_else(|| {
                    super::super::behavior_docking_execution::find_named_descendant_global_transform(
                        actor,
                        subject_node,
                        &children,
                        &named_nodes,
                    )
                    .map(GlobalTransform::compute_transform)
                })
                .or_else(|| {
                    actor_transforms
                        .get(actor)
                        .ok()
                        .map(|actor| Transform::from_translation(actor.translation()))
                })?;
            let presentation = if object.prefab == AssetId::default() {
                None
            } else {
                let handle = pending.prefab.as_ref()?;
                Some((handle, prefabs.get(handle)?))
            };
            let id = ids.allocate(world.root).ok()?;
            let transform =
                transform.mul_transform(Transform::from_scale(Vec3::splat(object.prefab_scale)));
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
        let Ok((mut task, pending, _)) = actors.get_mut(actor) else {
            continue;
        };
        if failed.contains(&actor)
            || !pending.owns(&task)
            || pending.request.is_none()
            || pending.request != event.explicit_clip_request_id
        {
            continue;
        }
        if pending.spawned {
            advance_behavior_task_to_next_action(&mut task, clock.tick);
        } else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
        commands
            .entity(actor)
            .remove::<PendingBehaviorObjectSpawn>();
    }
}
