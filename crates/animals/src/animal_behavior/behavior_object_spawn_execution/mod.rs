//! One authored spawn action owns its asset demand and animation-key delivery.

mod at_actor;
mod delivery;
mod preparation;
mod spawn_pose;
mod types;

use at_actor::spawn_behavior_objects_at_actor;
use delivery::deliver_behavior_object_spawns;
use preparation::prepare_behavior_object_spawns;
use types::{PendingAtActorSpawnPrefab, PendingBehaviorObjectSpawn};

use avian3d::prelude::RigidBody;
use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    platform::collections::HashSet,
    prelude::*,
};
use openzt2_game_data::{
    animation::animation_text_key::AuthoredAnimationTextAction,
    world_definitions::world_objects::WorldObjectPropertyFlags, AssetId,
};

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    {
        scene_prefab::ScenePrefabAsset,
        world_definitions::{
            world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
            world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
        },
    },
};
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequestRejected;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommand;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationObjectCommandDelivery;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::maintenance::maintenance_types::{AnimalHabitatWaste, LooseLitterWaste};

use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorSpawnObject, RunBehaviorSpawnObjectAtActor,
};

#[derive(EntityEvent)]
struct ResumePrefabSpawn {
    #[event_target]
    actor: Entity,
}

fn resume_prefab_spawn(event: On<ResumePrefabSpawn>) -> Option<Entity> {
    Some(event.actor)
}

#[allow(clippy::too_many_arguments)]
fn wake_prefab_spawn_actions(
    assets: Res<WorldDefinitions>,
    mut definition_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut loaded: MessageReader<AssetEvent<ScenePrefabAsset>>,
    mut failed: MessageReader<AssetLoadFailedEvent<ScenePrefabAsset>>,
    at_actor: Query<(Entity, &PendingAtActorSpawnPrefab)>,
    animated: Query<(Entity, &PendingBehaviorObjectSpawn)>,
    unresolved: Query<
        Entity,
        Or<(
            (
                With<RunBehaviorSpawnObjectAtActor>,
                Without<PendingAtActorSpawnPrefab>,
            ),
            (
                With<RunBehaviorSpawnObject>,
                Without<PendingBehaviorObjectSpawn>,
            ),
        )>,
    >,
    mut commands: Commands,
) {
    let definitions_changed = assets.is_changed()
        || definition_events.read().any(|event| {
            matches!(
                event,
                AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
            )
        });
    definition_events.clear();
    let settled = loaded
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .chain(failed.read().map(|event| event.id))
        .collect::<HashSet<_>>();
    if definitions_changed {
        for actor in &unresolved {
            commands.trigger(ResumePrefabSpawn { actor });
        }
    }
    if settled.is_empty() {
        return;
    }
    for (actor, pending) in &at_actor {
        if settled.contains(&pending.prefab.id()) {
            commands.trigger(ResumePrefabSpawn { actor });
        }
    }
    for (actor, pending) in &animated {
        if pending.request.is_none()
            && pending
                .prefab
                .as_ref()
                .is_some_and(|handle| settled.contains(&handle.id()))
        {
            commands.trigger(ResumePrefabSpawn { actor });
        }
    }
}

fn release_obsolete_spawn_demand(
    removed: On<Remove<(RunBehaviorSpawnObjectAtActor, RunBehaviorSpawnObject)>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<(PendingAtActorSpawnPrefab, PendingBehaviorObjectSpawn)>();
}

pub(super) fn register_object_spawn_execution(app: &mut App) {
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::activated_behavior_actor;

    use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
    app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorSpawnObjectAtActor>.pipe(spawn_behavior_objects_at_actor));
    app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorSpawnObject>.pipe(prepare_behavior_object_spawns));
    app.add_observer(resume_prefab_spawn.pipe(spawn_behavior_objects_at_actor))
        .add_observer(resume_prefab_spawn.pipe(prepare_behavior_object_spawns))
        .add_observer(release_obsolete_spawn_demand)
        .add_systems(
            FixedUpdate,
            wake_prefab_spawn_actions
                .in_set(BehaviorStepFixedSet::Think)
                .run_if(in_state(GamePhase::InGame)),
        )
        .add_systems(
            FixedUpdate,
            deliver_behavior_object_spawns
                .in_set(FixedGameSet::Act)
                .in_set(AnimationObjectCommandDelivery)
                .run_if(in_state(GamePhase::InGame)),
        );
}
