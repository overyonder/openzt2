use super::wander::BehaviorGroundLocomotionSpeed;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorDockNowAction {
    pub subject_node: Option<String>,
    pub target_node: Option<String>,
    pub target_animation: Option<String>,
    pub reservation: Option<AssetId>,
    pub redock: bool,
    pub parent_to_target: bool,
    pub unparent: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorDockSplineAction {
    pub subject_node: Option<String>,
    pub target_node: Option<String>,
    pub target_animation: Option<String>,
    pub speed: BehaviorGroundLocomotionSpeed,
    pub ignore_target_collision: bool,
    pub use_buoy_nodes: bool,
    pub redock: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorFollowEntityAction {
    pub subject_node: Option<String>,
    pub target_node: Option<String>,
    pub speed: BehaviorGroundLocomotionSpeed,
    pub play_seconds: [f32; 2],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorSplashGuestAction {
    pub animation: Option<String>,
    pub particle: Option<String>,
    pub direction: Option<[f32; 3]>,
    pub interrupts_animation: bool,
    pub ground_fit: Option<bool>,
    pub from_water_surface: bool,
    pub tokens: Vec<super::token::BehaviorSelfTokenDelivery>,
    pub show_event: Option<super::show_event::BehaviorShowEventAction>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorUsePortalAction {
    pub animation: Option<String>,
    pub speed: BehaviorGroundLocomotionSpeed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorDockTankWallAction {
    pub subject_node: Option<String>,
    pub speed: BehaviorGroundLocomotionSpeed,
    pub height_above_ground: Option<f32>,
    pub depth_below_water: Option<f32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorStaffTrainAnimalAction {
    pub increment: f32,
    pub summon_timeout: f32,
    pub trick_timeout: f32,
    pub require_platform: bool,
    pub go_to_platform: Option<AssetId>,
    pub prepare: Option<AssetId>,
    pub summon: Option<AssetId>,
    pub await_animal: Option<AssetId>,
    pub train: Option<AssetId>,
    pub perform_trick: Option<AssetId>,
    pub await_trick: Option<AssetId>,
    pub reward: Option<AssetId>,
    pub cleanup: Option<AssetId>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorAddSubjectAsDiseaseTargetAction {
    pub disease: AssetId,
    pub types: Vec<AssetId>,
    pub attributes: Vec<(AssetId, bool)>,
    pub immediate: bool,
}
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct BehaviorFallAction {
    pub speed_mps: f32,
    pub airborne: Option<String>,
    pub land_impact: Option<String>,
    pub water_impact: Option<String>,
}
