use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use crate::plugins::{
    camera::world_pointer_ray_types::WorldPointerRay,
    placement::placed_object_types::PlacedObjectDefinitionReference,
};

use super::{
    construction_interaction_types::{ConstructionCursor, DeleteHoverTarget},
    construction_tool_and_placement_policy_types::ConstructionTool,
};

/// Walks from the hovered collider to its owning placeable entity.
pub(super) fn update_delete_hover_target(
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    ray: Res<WorldPointerRay>,
    spatial_query: SpatialQuery,
    parents: Query<&ChildOf>,
    placeables: Query<(), With<PlacedObjectDefinitionReference>>,
    cursors: Query<(Entity, Option<&DeleteHoverTarget>), With<ConstructionCursor>>,
) {
    let Ok((cursor, previous)) = cursors.single() else {
        return;
    };
    let target = (*tool == ConstructionTool::Delete)
        .then_some(ray.0)
        .flatten()
        .and_then(|ray| {
            spatial_query.cast_ray_predicate(
                ray.origin,
                ray.direction,
                f32::MAX,
                false,
                &SpatialQueryFilter::DEFAULT,
                &|entity| placeable_ancestor(entity, &parents, &placeables).is_some(),
            )
        })
        .and_then(|hit| placeable_ancestor(hit.entity, &parents, &placeables));
    if previous.map(|target| target.0) == target {
        return;
    }
    let mut cursor = commands.entity(cursor);
    if let Some(target) = target {
        cursor.insert(DeleteHoverTarget(target));
    } else {
        cursor.remove::<DeleteHoverTarget>();
    }
}

fn placeable_ancestor(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    placeables: &Query<(), With<PlacedObjectDefinitionReference>>,
) -> Option<Entity> {
    loop {
        if placeables.contains(entity) {
            return Some(entity);
        }
        entity = parents.get(entity).ok()?.parent();
    }
}
