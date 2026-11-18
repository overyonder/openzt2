use crate::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::behavior_task_execution_types::BehaviorTaskReturnFrame;
use bevy::{
    ecs::{lifecycle::HookContext, relationship::RelationshipTarget, world::DeferredWorld},
    prelude::*,
};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

/// One `ZTBehPlaceTarget` instruction in progress. Behavior execution owns
/// the task frames; the placement owner sites, pays for and creates the object.
#[derive(Component)]
#[component(on_discard = release_behavior_target_placement_site)]
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
    },
    /// The set returned; the placement owner pays for and creates the object.
    PuttingDown {
        site: BehaviorTargetPlacementSite,
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

impl BehaviorTargetPlacementSite {
    pub fn uses_current_prefab(
        &self,
        definitions: WorldDefinitionsView<'_>,
        definition: AssetId,
    ) -> bool {
        definitions.find_object(definition).is_some_and(|object| {
            self.transform.scale == Vec3::splat(object.prefab_scale)
                && definitions
                    .scene(object.prefab)
                    .is_some_and(|prefab| prefab == self.prefab)
        })
    }
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
#[relationship(relationship_target = BehaviorTargetPlacementMarkers)]
pub struct BehaviorTargetPlacementSiteOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = BehaviorTargetPlacementSiteOf, linked_spawn)]
pub struct BehaviorTargetPlacementMarkers(Vec<Entity>);

fn release_behavior_target_placement_site(mut world: DeferredWorld, context: HookContext) {
    let marker = world
        .get::<BehaviorTargetPlacementMarkers>(context.entity)
        .and_then(|markers| markers.iter().next());
    if let Some(marker) = marker {
        world.commands().entity(marker).try_despawn();
    }
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

    pub fn site(&self) -> Option<&BehaviorTargetPlacementSite> {
        match &self.stage {
            BehaviorTargetPlacementStage::Sited(site)
            | BehaviorTargetPlacementStage::Delivering { site }
            | BehaviorTargetPlacementStage::PuttingDown { site, .. } => Some(site),
            _ => None,
        }
    }
}
