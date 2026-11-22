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
use bevy::{asset::LoadState, prelude::*};
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

pub(super) fn register_object_spawn_execution(app: &mut App) {
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        activated_behavior_actor, update_waiting_behavior_actors,
    };

    use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
    app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorSpawnObjectAtActor>.pipe(spawn_behavior_objects_at_actor));
    app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorSpawnObject>.pipe(prepare_behavior_object_spawns));
    app.add_systems(
        FixedUpdate,
        update_waiting_behavior_actors
            .pipe(prepare_behavior_object_spawns)
            .in_set(BehaviorStepFixedSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    )
    .add_systems(
        FixedUpdate,
        update_waiting_behavior_actors
            .pipe(spawn_behavior_objects_at_actor)
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
