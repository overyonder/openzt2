//! One authored spawn action owns its asset demand and animation-key delivery.

mod delivery;
mod preparation;
mod types;

use delivery::deliver_behavior_object_spawns;
use preparation::prepare_behavior_object_spawns;
use types::PendingBehaviorObjectSpawn;

use avian3d::prelude::RigidBody;
use bevy::{asset::LoadState, prelude::*};
use openzt2_game_data::{
    animation::animation_text_key::AuthoredAnimationTextAction,
    behavior::action_record::BehaviorAction,
    world_definitions::world_objects::WorldObjectPropertyFlags, AssetId,
};

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use crate::{
    application_schedule::FixedGameSet,
    plugins::{
        animation_graph::{
            animation_graph_playback_message_types::{
                AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
            },
            animation_presentation_relationship_types::AnimationPresentationOwner,
        },
        animation_playback::{
            animation_event_message_types::{
                AnimationCompleted, AnimationObjectCommand, AnimationObjectCommandDelivery,
            },
            animation_joint_target_types::AnimationJointTarget,
            animation_playback_controller_types::{
                AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
            },
        },
        behavior_task_execution_types::{
            advance_behavior_task_to_next_action, find_current_behavior_task_action,
            BehaviorTaskExecutionState, BehaviorTaskReturnFrame, PendingBehaviorTaskFailure,
        },
        information::entity_selection_types::Inspectable,
        maintenance::maintenance_types::AnimalHabitatWaste,
        simulation_time::simulation_clock_types::ZooClock,
        world_spawn::{
            persistent_id_types::PersistentIdAllocator,
            prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance,
            world_membership_types::{DefinitionId, WorldMember},
        },
    },
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    {
        behavior::behavior_asset_types::BehaviorDocumentAsset,
        scene_prefab::ScenePrefabAsset,
        world_definitions::{
            world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
            world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
        },
    },
};

pub(super) fn register_object_spawn_execution(app: &mut App) {
    app.add_systems(FixedUpdate, prepare_behavior_object_spawns
        .in_set(FixedGameSet::Think)
        .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
        .run_if(in_state(GamePhase::InGame)))
        .add_systems(FixedUpdate, deliver_behavior_object_spawns
            .in_set(FixedGameSet::Act)
            .in_set(AnimationObjectCommandDelivery)
            .run_if(in_state(GamePhase::InGame)));
}
