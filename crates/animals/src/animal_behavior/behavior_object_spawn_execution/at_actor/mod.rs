use super::*;
#[cfg(test)]
mod tests;
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::guests::guest_simulation_types::GuestRng;

/// Spawns the object of a node- and clip-less `BFBehSpawn` beside its actor,
/// such as the `TrashGround` a guest drops, once its model has loaded.
pub(super) fn spawn_behavior_objects_at_actor(
    mut commands: Commands,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    server: Res<AssetServer>,
    clock: Res<ZooClock>,
    mut ids: ResMut<PersistentIdAllocator>,
    pose_validation: super::spawn_pose::SpawnPoseValidation,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &WorldMember,
            Option<&mut BehaviorRandomStream>,
            Option<&mut GuestRng>,
            Option<&PendingAtActorSpawnPrefab>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let Some(view) = definitions.get(&definition_assets) else {
        return;
    };
    for (actor, mut task, world, animal_random, guest_random, pending) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::SpawnObjectAtActor { entity, pose }) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let Some(object) = view.find_object(*entity) else {
            if definitions.is_complete() {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            continue;
        };
        let presentation = if object.prefab == AssetId::default() {
            None
        } else {
            let Some(handle) = pending
                .filter(|pending| pending.object == object.id)
                .map(|pending| pending.prefab.clone())
                .or_else(|| view.scene(object.prefab))
            else {
                if definitions.is_complete() {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
                continue;
            };
            if matches!(
                server.get_load_state(handle.id()),
                Some(LoadState::Failed(_))
            ) {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
            // Wait for the model rather than spawning an invisible object,
            // holding its handle so the load isn't cancelled meanwhile.
            let Some(prefab) = prefabs.get(&handle) else {
                commands.entity(actor).insert(PendingAtActorSpawnPrefab {
                    object: object.id,
                    prefab: handle,
                });
                continue;
            };
            Some((handle, prefab))
        };
        let random = match (animal_random, guest_random) {
            (Some(random), _) => &mut random.into_inner().0,
            (None, Some(random)) => &mut random.into_inner().0,
            (None, None) => {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
        };
        let (Some(actor_pose), Ok(id)) =
            (pose_validation.actor_pose(actor), ids.allocate(world.root))
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let transform = pose_validation
            .select_spawn_pose(
                actor, object.id, actor_pose, actor_pose, *pose, view, random,
            )
            .mul_transform(Transform::from_scale(Vec3::splat(object.prefab_scale)));
        let spawned = if let Some((handle, prefab)) = presentation {
            spawn_loaded_scene_prefab_as_world_instance(
                &mut commands,
                prefab,
                handle,
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
            .contains_all(WorldObjectPropertyFlags::LOOSE_LITTER)
        {
            commands.entity(spawned).insert(LooseLitterWaste {
                uncontained_waste_units: 1,
            });
        }
        if pending.is_some() {
            commands.entity(actor).remove::<PendingAtActorSpawnPrefab>();
        }
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}
