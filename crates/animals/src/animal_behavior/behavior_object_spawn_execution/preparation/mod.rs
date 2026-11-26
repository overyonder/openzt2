use super::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorSpawnObject, RunningBehaviorInstruction,
};

pub(super) fn prepare_behavior_object_spawns(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    server: Res<AssetServer>,
    clock: Single<&ZooClock>,
    mut requests: MessageWriter<AnimationClipPlaybackRequest>,
    mut actors: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorSpawnObject,
            ),
            Option<&mut PendingBehaviorObjectSpawn>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let Some(view) = definitions.get(&definition_assets) else {
        return;
    };
    for (actor, (task, instruction, running_action), pending) in
        behavior_action_actors(&mut actors, activation.0)
    {
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if pending
            .as_ref()
            .is_some_and(|pending| !pending.owns(task, instruction))
        {
            commands
                .entity(actor)
                .remove::<PendingBehaviorObjectSpawn>();
            continue;
        }
        if task.next_action_tick > clock.tick
            || pending
                .as_ref()
                .is_some_and(|pending| pending.request.is_some())
        {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let RunBehaviorSpawnObject {
            entity, animation, ..
        } = running_action;
        let Some(object) = view.find_object(*entity) else {
            if definitions.is_complete() {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            continue;
        };
        let prefab = if object.prefab == AssetId::default() {
            None
        } else {
            let Some(handle) = pending
                .as_ref()
                .and_then(|pending| pending.prefab.clone())
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
            Some(handle)
        };
        let ready = prefab
            .as_ref()
            .is_none_or(|handle| prefabs.get(handle).is_some());
        let request = ready.then(|| {
            requests
                .write(AnimationClipPlaybackRequest {
                    animation_subject_entity: actor,
                    animation_clip_asset_key: animation.clone(),
                    blend_duration_milliseconds: 0,
                    playback_speed_permille: 1000,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                    action_settings: None,
                })
                .id
        });
        if let Some(mut pending) = pending {
            if request.is_some() {
                pending.request = request;
            }
        } else {
            commands.entity(actor).insert(PendingBehaviorObjectSpawn {
                instruction: instruction.id(),
                target: task.target,
                prefab,
                request,
                spawned: false,
            });
        }
    }
}
