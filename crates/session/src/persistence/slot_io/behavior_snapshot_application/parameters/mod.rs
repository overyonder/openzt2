use super::super::behavior_snapshot_entity_references::BehaviorSnapshotEntities;
use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    prelude::*,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_zoo_state::{
    animal_behavior::interaction_container_membership::InteractionContainerMemberships,
    behavior_task_execution_types::{BehaviorTaskGeneration, BehaviorTaskReturnFrame},
    locomotion::locomotion_types::{
        AnimalTerrainWaterContact, ContactSteering, LocomotionCapacity, LocomotionMode,
        MotionProgress, NavAgent, NavigationRequestGeneration, Route, Steering, SwimLayer,
        Velocity,
    },
};

#[derive(SystemParam)]
pub(in crate::persistence) struct BehaviorSnapshotApplication<'w, 's> {
    pub(super) actors: ParamSet<
        'w, 's,
        (
            BehaviorSnapshotEntities<'w, 's>,
            Query<'w, 's, RestoredBehaviorNavigation>,
        ),
    >,
    pub(super) documents: Res<'w, Assets<BehaviorDocumentAsset>>,
    pub(super) server: Res<'w, AssetServer>,
    pub(super) executions: Query<'w, 's, &'static mut BehaviorTaskGeneration>,
    pub(super) requests: Query<'w, 's, &'static mut NavigationRequestGeneration>,
    pub(super) capacity: Query<'w, 's, &'static LocomotionCapacity>,
    pub(super) memberships: InteractionContainerMemberships<'w, 's>,
    pub(super) definitions: Res<'w, openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>,
    pub(super) definition_assets: Res<'w, Assets<openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>>,
    pub(super) route_requests: MessageWriter<'w, openzt2_zoo_state::locomotion::locomotion_types::NavigateTo>,
    pub(super) pursuit_requests: MessageWriter<'w, openzt2_zoo_state::locomotion::locomotion_types::PursueEntity>,
    pub(super) arrivals: MessageWriter<'w, openzt2_zoo_state::locomotion::locomotion_types::Arrived>,
    pub(super) failures: MessageWriter<'w, openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed>,
}

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct RestoredBehaviorNavigation {
    pub(super) agent: Option<&'static mut NavAgent>,
    pub(super) mode: Option<&'static mut LocomotionMode>,
    pub(super) route: Option<&'static mut Route>,
    pub(super) steering: Option<&'static mut Steering>,
    pub(super) contact_steering: Option<&'static mut ContactSteering>,
    pub(super) progress: Option<&'static mut MotionProgress>,
    pub(super) velocity: Option<&'static mut Velocity>,
    pub(super) linear_velocity: Option<&'static mut avian3d::prelude::LinearVelocity>,
    pub(super) swim_layer: Option<&'static mut SwimLayer>,
    pub(super) water_contact: Option<&'static mut AnimalTerrainWaterContact>,
    pub(super) transform: Option<&'static GlobalTransform>,
    pub(super) has_rigid_body: Has<avian3d::prelude::RigidBody>,
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
    pub(super) job_frame: Option<BehaviorTaskReturnFrame>,
    pub(super) synchronized_origin: Option<BehaviorTaskReturnFrame>,
}
