use crate::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::behavior_task_execution_types::BehaviorTaskReturnFrame;
use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

/// One `ZTBehPlaceTarget` instruction in progress. Behavior execution owns
/// the task frames; the placement owner sites, pays for and creates the object.
#[derive(Component)]
pub struct PendingBehaviorTargetPlacement {
    pub execution_id: u64,
    /// The task frame at the placement instruction, re-entered when the
    /// placement set returns.
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub root: Entity,
    /// The task target the object is placed for, restored after the set.
    pub recipient: Entity,
    pub definition: AssetId,
    pub definition_revision: u64,
    pub stage: BehaviorTargetPlacementStage,
}

#[derive(Clone)]
pub enum BehaviorTargetPlacementStage {
    /// The placement owner chooses and validates the site. It holds the
    /// object's prefab so the load it demands is not dropped between ticks.
    Siting {
        prefab: Option<Handle<ScenePrefabAsset>>,
    },
    /// A valid site was chosen; the object stays in the holding container.
    Sited(BehaviorTargetPlacementSite),
    /// The subject plays the placement set toward the site marker.
    Delivering {
        site: BehaviorTargetPlacementSite,
        marker: Entity,
    },
    /// The set returned; the placement owner pays for and creates the object.
    PuttingDown {
        site: BehaviorTargetPlacementSite,
        marker: Entity,
        payment: BehaviorTargetPlacementPaymentState,
    },
    Placed,
    Rejected,
}

#[derive(Clone)]
pub struct BehaviorTargetPlacementSite {
    pub transform: Transform,
    pub prefab: Handle<ScenePrefabAsset>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BehaviorTargetPlacementPaymentState {
    Unrequested,
    /// The settlement operation; the settled amount is returned if the
    /// placement cannot complete.
    Requested(Entity),
}

/// The point a placement set docks at while the object is still held.
#[derive(Component)]
pub struct BehaviorTargetPlacementSiteMarker {
    pub actor: Entity,
}

impl PendingBehaviorTargetPlacement {
    /// True while the task is at the instruction or inside its placement set.
    pub fn owns(&self, task: &BehaviorTaskExecutionState, world: &WorldMember) -> bool {
        self.execution_id == task.execution_id
            && self.root == world.root
            && task.stack.len() >= self.depth
            && task.is_at_or_nested_under(&self.origin, self.depth)
    }

    /// True when the task is exactly at the instruction, not inside its set.
    pub fn is_current(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.stack.len() == self.depth && task.is_at_or_nested_under(&self.origin, self.depth)
    }

    pub const fn marker(&self) -> Option<Entity> {
        match self.stage {
            BehaviorTargetPlacementStage::Delivering { marker, .. }
            | BehaviorTargetPlacementStage::PuttingDown { marker, .. } => Some(marker),
            _ => None,
        }
    }
}
