use bevy::{ecs::{lifecycle::HookContext, world::DeferredWorld}, prelude::*};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::{world_definitions::object_placement::PlaceableDefinition, AssetId};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use crate::behavior_task_execution_types::{
    continuations::BehaviorContinuationFrames, BehaviorTaskExecutionState, BehaviorTaskReturnFrame,
};

#[derive(Component)]
#[component(on_discard = remove_travel_site)]
pub struct BehaviorTargetPlacement {
    pub execution_id: u64,
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub root: Entity,
    pub definition: AssetId,
}

#[derive(Component)]
#[relationship(relationship_target = BehaviorPlacements)]
pub struct BehaviorPlacementOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = BehaviorPlacementOf, linked_spawn)]
pub struct BehaviorPlacements(Entity);

#[derive(Component)]
#[relationship(relationship_target = BehaviorPlacementRecipients)]
pub struct BehaviorPlacementRecipient(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = BehaviorPlacementRecipient)]
pub struct BehaviorPlacementRecipients(Vec<Entity>);

#[derive(Component, Default)]
pub struct BehaviorPlacementSiting(pub Option<Handle<ScenePrefabAsset>>);

#[derive(Component, Clone)]
pub struct BehaviorTargetPlacementSite {
    pub rules: PlaceableDefinition,
    pub transform: Transform,
    pub prefab: Handle<ScenePrefabAsset>,
}

#[derive(Component)]
pub struct BehaviorPlacementDelivering;

#[derive(Component)]
pub struct BehaviorPlacementPutDown;

#[derive(Component)]
pub struct BehaviorPlacementAwaitingPayment(pub Entity);

#[derive(Component)]
pub struct BehaviorPlacementResult(pub bool);

#[derive(Component)]
pub struct BehaviorPlacementTravelSite {
    pub operation: Entity,
    pub point: Entity,
}

impl BehaviorTargetPlacementSite {
    pub fn uses_current_prefab(
        &self,
        definitions: WorldDefinitionsView<'_>,
        definition: AssetId,
    ) -> bool {
        crate::placement::object_placement_definition_queries::resolve_object_placeable_definition(
            definitions,
            definition,
        )
        .is_some_and(|rules| *rules == self.rules)
            && definitions.find_object(definition).is_some_and(|object| {
                self.transform.scale == Vec3::splat(object.prefab_scale)
                    && definitions
                        .scene(object.prefab)
                        .is_some_and(|prefab| prefab == self.prefab)
            })
    }
}

impl BehaviorTargetPlacement {
    pub fn owns(
        &self,
        actor: Entity,
        task: &BehaviorTaskExecutionState,
        world: &WorldMember,
        frames: &BehaviorContinuationFrames,
    ) -> bool {
        self.execution_id == task.execution_id
            && self.root == world.root
            && frames.owns(actor, task, &self.origin, self.depth)
    }

    pub fn is_current(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.is_at_exact_frame(&self.origin, self.depth)
    }
}

pub fn travel_target(
    task: &BehaviorTaskExecutionState,
    site: Option<&BehaviorPlacementTravelSite>,
) -> Option<Entity> {
    site.map(|site| site.point).or(task.target)
}

fn remove_travel_site(mut world: DeferredWorld, context: HookContext) {
    let Some(actor) = world
        .get::<BehaviorPlacementOf>(context.entity)
        .map(|owner| owner.0)
    else {
        return;
    };
    let operation = context.entity;
    world.commands().entity(actor).queue_silenced(
        move |mut actor: bevy::ecs::world::EntityWorldMut<'_>| {
            if actor
                .get::<BehaviorPlacementTravelSite>()
                .is_some_and(|site| site.operation == operation)
            {
                actor.remove::<BehaviorPlacementTravelSite>();
            }
        },
    );
}
