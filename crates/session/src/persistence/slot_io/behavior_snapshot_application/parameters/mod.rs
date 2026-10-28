use super::super::behavior_snapshot_entity_references::BehaviorSnapshotEntities;
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_zoo_state::{
    behavior_task_execution_types::{BehaviorTaskExecutionSequence, BehaviorTaskReturnFrame},
    locomotion::locomotion_types::{LocomotionCapacity, NavigationRequestSequence},
};

#[derive(SystemParam)]
pub(in crate::persistence) struct BehaviorSnapshotApplication<'w, 's> {
    pub(super) entities: BehaviorSnapshotEntities<'w, 's>,
    pub(super) documents: Res<'w, Assets<BehaviorDocumentAsset>>,
    pub(super) server: Res<'w, AssetServer>,
    pub(super) executions: ResMut<'w, BehaviorTaskExecutionSequence>,
    pub(super) requests: ResMut<'w, NavigationRequestSequence>,
    pub(super) capacity: Option<Res<'w, LocomotionCapacity>>,
    pub(super) occupancy: ResMut<'w, openzt2_zoo_state::animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy>,
    pub(super) definitions: Res<'w, openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>,
    pub(super) definition_assets: Res<'w, Assets<openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>>,
    pub(super) route_requests: MessageWriter<'w, openzt2_zoo_state::locomotion::locomotion_types::NavigateTo>,
    pub(super) pursuit_requests: MessageWriter<'w, openzt2_zoo_state::locomotion::locomotion_types::PursueEntity>,
    pub(super) arrivals: MessageWriter<'w, openzt2_zoo_state::locomotion::locomotion_types::Arrived>,
    pub(super) failures: MessageWriter<'w, openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed>,
}

pub(super) struct PreparedBehaviorActor {
    pub(super) actor: Entity,
    pub(super) current: BehaviorTaskReturnFrame,
    pub(super) stack: arrayvec::ArrayVec<
        BehaviorTaskReturnFrame,
        {
            openzt2_zoo_state::behavior_task_execution_types::MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT
        },
    >,
    pub(super) move_origin: Option<BehaviorTaskReturnFrame>,
    pub(super) dock_origin: Option<BehaviorTaskReturnFrame>,
    pub(super) pursuit_origin: Option<BehaviorTaskReturnFrame>,
    pub(super) job_frame: Option<BehaviorTaskReturnFrame>,
    pub(super) synchronized_origin: Option<BehaviorTaskReturnFrame>,
}
