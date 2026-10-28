use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};
use openzt2_zoo_state::{
    animal_behavior::behavior_pursuit_types::BehaviorPursuitLeg,
    behavior_task_execution_types::BehaviorTaskExecutionPhase,
    locomotion::locomotion_types::{
        ContactSteering, Destination, LocomotionMode, MotionProgress, NavAgent, Route, SwimLayer,
        Velocity,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(super) enum BehaviorSnapshotEntityReference {
    Persistent(u64),
    PathTile([i32; 3]),
    TerrainWater {
        terrain: String,
        source_region: usize,
    },
    Habitat([i32; 2]),
    TankCleaningSite {
        tank: [i32; 2],
        position: [u32; 3],
    },
    SelfToken {
        recipient: u64,
        name: AssetId,
        available_at: u64,
        expires_at: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorFrameSnapshot {
    pub document: String,
    pub program: AssetId,
    pub subjects: Vec<AssetId>,
    pub phase: BehaviorTaskExecutionPhase,
    pub action: usize,
    pub repetitions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorTaskSnapshot {
    pub execution_id: u64,
    pub current: BehaviorFrameSnapshot,
    pub target: Option<BehaviorSnapshotEntityReference>,
    pub object: Option<BehaviorSnapshotEntityReference>,
    pub next_action_tick: u64,
    pub interaction_slot: Option<usize>,
    pub stack: Vec<BehaviorFrameSnapshot>,
    pub score: Option<BehaviorTaskScoreSnapshot>,
    pub failing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorTaskScoreSnapshot {
    pub execution_id: u64,
    pub template: Option<u32>,
    pub linked: Option<u32>,
    pub selected_candidate: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorNavigationCompletionSnapshot {
    pub origin: BehaviorFrameSnapshot,
    pub stack_depth: usize,
    pub request_id: u64,
    pub target: Option<BehaviorSnapshotEntityReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorPursuitSnapshot {
    pub execution_id: u64,
    pub origin: BehaviorFrameSnapshot,
    pub stack_depth: usize,
    pub poll_remaining_seconds: f32,
    pub leg: BehaviorPursuitLeg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorDockingSnapshot {
    pub request_id: u64,
    pub target: BehaviorSnapshotEntityReference,
    pub point: [f32; 3],
    pub forward: [f32; 3],
    pub radius_m: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorOwnedNavigationSnapshot {
    pub agent: NavAgent,
    pub mode: LocomotionMode,
    pub swim_layer: Option<SwimLayer>,
    pub water_contact:
        Option<openzt2_zoo_state::locomotion::locomotion_types::AnimalTerrainWaterContact>,
    pub velocity: Velocity,
    pub contact_steering: Option<ContactSteering>,
    pub progress: Option<MotionProgress>,
    pub destination: Option<Destination>,
    pub route: Route,
    pub pursuit_target: Option<BehaviorSnapshotEntityReference>,
    pub docking: Option<BehaviorDockingSnapshot>,
    pub move_completion: Option<BehaviorNavigationCompletionSnapshot>,
    pub docking_completion: Option<BehaviorNavigationCompletionSnapshot>,
    pub pursuit: Option<BehaviorPursuitSnapshot>,
    pub pending_results: Vec<BehaviorNavigationResultSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum BehaviorNavigationResultSnapshot {
    Arrived {
        request_id: u64,
        target: Option<BehaviorSnapshotEntityReference>,
    },
    Failed {
        request_id: u64,
        reason: openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure,
    },
}

impl BehaviorNavigationResultSnapshot {
    pub(super) fn request_id(&self) -> u64 {
        match self {
            Self::Arrived { request_id, .. } | Self::Failed { request_id, .. } => *request_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorInteractionMembershipSnapshot {
    pub container: BehaviorSnapshotEntityReference,
    pub definition: AssetId,
    pub slot: usize,
    pub admission_order: usize,
    pub docked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorNaturalWaterTargetSnapshot {
    pub region: BehaviorSnapshotEntityReference,
    pub water: [f32; 3],
    pub approach: [f32; 3],
    pub depth: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorStaffJobSnapshot {
    pub kind: StaffJobKind,
    pub target: BehaviorSnapshotEntityReference,
    pub urgency: u32,
    pub token: Option<AssetId>,
    pub document: String,
    pub program: AssetId,
    pub execution_id: u64,
    pub progressing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorActorSnapshot {
    pub persistent_identifier: u64,
    pub task: BehaviorTaskSnapshot,
    pub navigation: Option<BehaviorOwnedNavigationSnapshot>,
    pub water_target: Option<BehaviorNaturalWaterTargetSnapshot>,
    pub staff_job: Option<BehaviorStaffJobSnapshot>,
    pub random: Option<[u64; 2]>,
    pub synchronized: Option<BehaviorSynchronizedSnapshot>,
    pub membership: Option<BehaviorInteractionMembershipSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BehaviorSynchronizedSnapshot {
    pub execution_id: u64,
    pub target_execution_id: u64,
    pub target: BehaviorSnapshotEntityReference,
    pub origin: BehaviorFrameSnapshot,
    pub depth: usize,
    pub target_outcome: Option<bool>,
}
