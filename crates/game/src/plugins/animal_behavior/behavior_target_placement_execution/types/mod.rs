use crate::plugins::{
    behavior_task_execution_types::{BehaviorTaskExecutionState, BehaviorTaskReturnFrame},
    world_spawn::world_membership_types::WorldMember,
};
use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_data::AssetId;

/// One `ZTBehPlaceTarget` instruction in progress. Behavior execution owns
/// the task frames; the placement owner sites, pays for and creates the object.
#[derive(Component)]
pub(crate) struct PendingBehaviorTargetPlacement {
    pub(crate) execution_id: u64,
    /// The task frame at the placement instruction, re-entered when the
    /// placement set returns.
    pub(crate) origin: BehaviorTaskReturnFrame,
    pub(crate) depth: usize,
    pub(crate) root: Entity,
    /// The task target the object is placed for, restored after the set.
    pub(crate) recipient: Entity,
    pub(crate) definition: AssetId,
    pub(crate) definition_revision: u64,
    pub(crate) stage: BehaviorTargetPlacementStage,
}

#[derive(Clone)]
pub(crate) enum BehaviorTargetPlacementStage {
    /// The placement owner chooses and validates the site.
    Siting,
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
pub(crate) struct BehaviorTargetPlacementSite {
    pub(crate) transform: Transform,
    pub(crate) prefab: Handle<ScenePrefabAsset>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BehaviorTargetPlacementPaymentState {
    Unrequested,
    /// The settlement operation; the settled amount is returned if the
    /// placement cannot complete.
    Requested(Entity),
}

/// The point a placement set docks at while the object is still held.
#[derive(Component)]
pub(crate) struct BehaviorTargetPlacementSiteMarker {
    pub(crate) actor: Entity,
}

impl PendingBehaviorTargetPlacement {
    /// True while the task is at the instruction or inside its placement set.
    pub(crate) fn owns(&self, task: &BehaviorTaskExecutionState, world: &WorldMember) -> bool {
        self.execution_id == task.execution_id
            && self.root == world.root
            && task.stack.len() >= self.depth
            && task.is_at_or_nested_under(&self.origin, self.depth)
    }

    /// True when the task is exactly at the instruction, not inside its set.
    pub(crate) fn is_current(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.stack.len() == self.depth && task.is_at_or_nested_under(&self.origin, self.depth)
    }

    pub(crate) const fn marker(&self) -> Option<Entity> {
        match self.stage {
            BehaviorTargetPlacementStage::Delivering { marker, .. }
            | BehaviorTargetPlacementStage::PuttingDown { marker, .. } => Some(marker),
            _ => None,
        }
    }
}
