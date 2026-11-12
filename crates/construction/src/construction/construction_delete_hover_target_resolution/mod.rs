use bevy::prelude::*;

use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyGrid, TopologyNode};
use openzt2_zoo_state::transport_tours::transport_topology_types::{
    TrackProfile, TrackSegment, TransportTrackJunction,
};
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionPiece;
use openzt2_zoo_state::transport_tours::transport_track_sale::TransportTrackSaleQueries;

use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::DeleteHoverTarget;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;

/// How close to a fence segment the pointer's terrain point must be for the
/// delete tool to take it. Fences carry no physics collider to hit.
const FENCE_DELETE_PICK_RADIUS_M: f32 = 1.0;

/// How close to a road piece's centre line the pointer's terrain point must
/// be for the delete tool to take it; a ground road is about 3 m wide and,
/// drawn as a terrain decal, has no mesh the pointer ray can select.
const TRACK_DELETE_PICK_RADIUS_M: f32 = 1.5;

/// Walks from the hovered collider to its owning placeable entity, or else
/// takes the fence segment under the pointer, which the delete tool removes
/// as in the original.
#[allow(clippy::too_many_arguments)]
pub(super) fn update_delete_hover_target(
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    ray: Res<WorldPointerRay>,
    mut mesh_ray_cast: MeshRayCast,
    parents: Query<&ChildOf>,
    placeables: Query<(), With<PlacedObjectDefinitionReference>>,
    cursors: Query<(Entity, &ConstructionCursor, Option<&DeleteHoverTarget>)>,
    fences: Query<(Entity, &FenceEdge)>,
    nodes: Query<&TopologyNode>,
    grid: Res<TopologyGrid>,
    track_pieces: Query<(Entity, &TransportTrackConstructionPiece)>,
    routes: Query<&TrackSegment>,
    (sales, sky_towers): (
        TransportTrackSaleQueries,
        Query<(Entity, &GlobalTransform), With<TransportTrackJunction>>,
    ),
    sky_routes: Query<(&TrackSegment, &TrackProfile)>,
) {
    let Ok((cursor_entity, cursor, previous)) = cursors.single() else {
        return;
    };
    let target = (*tool == ConstructionTool::Delete)
        .then_some(ray.0)
        .flatten()
        .and_then(|ray| {
            // Against meshes, as selection is: trees and plants have no colliders.
            let filter = |entity| placeable_ancestor(entity, &parents, &placeables).is_some();
            mesh_ray_cast
                .cast_ray(ray, &MeshRayCastSettings::default().with_filter(&filter))
                .first()
                .map(|(hit, _)| *hit)
        })
        .and_then(|hit| placeable_ancestor(hit, &parents, &placeables))
        .or_else(|| {
            (*tool == ConstructionTool::Delete && cursor.over_terrain)
                .then(|| nearest_fence(cursor.world.xz(), &fences, &nodes, *grid))
                .flatten()
        })
        .or_else(|| {
            (*tool == ConstructionTool::Delete)
                .then_some(ray.0)
                .flatten()
                .and_then(|ray| nearest_sky_tower(ray, &sky_towers, &sky_routes, &sales))
        })
        .or_else(|| {
            (*tool == ConstructionTool::Delete && cursor.over_terrain)
                .then(|| nearest_track_piece(cursor.world.xz(), &track_pieces, &routes, &sales))
                .flatten()
        });
    if previous.map(|target| target.0) == target {
        return;
    }
    let mut cursor = commands.entity(cursor_entity);
    if let Some(target) = target {
        cursor.insert(DeleteHoverTarget(target));
    } else {
        cursor.remove::<DeleteHoverTarget>();
    }
}

fn nearest_fence(
    point: Vec2,
    fences: &Query<(Entity, &FenceEdge)>,
    nodes: &Query<&TopologyNode>,
    grid: TopologyGrid,
) -> Option<Entity> {
    fences
        .iter()
        .filter_map(|(entity, fence)| {
            let [a, b] = [fence.a, fence.b].map(|node| {
                nodes
                    .get(node)
                    .ok()
                    .map(|node| grid.cell_translation(node.cell).xz())
            });
            let (a, b) = (a?, b?);
            let along = (point - a).dot(b - a) / (b - a).length_squared().max(f32::EPSILON);
            let distance = point.distance(a + (b - a) * along.clamp(0.0, 1.0));
            (distance <= FENCE_DELETE_PICK_RADIUS_M).then_some((entity, distance))
        })
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(entity, _)| entity)
}

/// The sky tower the pointer ray passes within 2 m of, along its height,
/// whose span the sale can remove.
fn nearest_sky_tower(
    ray: Ray3d,
    towers: &Query<(Entity, &GlobalTransform), With<TransportTrackJunction>>,
    routes: &Query<(&TrackSegment, &TrackProfile)>,
    sales: &TransportTrackSaleQueries,
) -> Option<Entity> {
    // ponytail: a fixed 12 m pick height, read the tower's height if tall towers miss.
    towers
        .iter()
        .filter(|(entity, _)| {
            routes.iter().any(|(route, profile)| {
                route.to == *entity && profile.kind == TransportationTrackKind::Sky
            }) && sales.terminal_route_edit(*entity).is_some()
        })
        .filter_map(|(entity, transform)| {
            let base = transform.translation();
            let distance = (0..=24)
                .map(|step| base + Vec3::Y * (12.0 * step as f32 / 24.0))
                .map(|point| {
                    let along = (point - ray.origin).dot(*ray.direction).max(0.0);
                    point.distance(ray.origin + *ray.direction * along)
                })
                .fold(f32::INFINITY, f32::min);
            (distance <= 2.0).then_some((entity, distance))
        })
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(entity, _)| entity)
}

fn nearest_track_piece(
    point: Vec2,
    pieces: &Query<(Entity, &TransportTrackConstructionPiece)>,
    routes: &Query<&TrackSegment>,
    sales: &TransportTrackSaleQueries,
) -> Option<Entity> {
    pieces
        .iter()
        // Only a leg the sale can remove, so the tool never marks one it can't.
        .filter(|(entity, _)| sales.terminal_route_edit(*entity).is_some())
        .filter_map(|(entity, piece)| {
            let route = routes.get(piece.route_segment).ok()?;
            let index = usize::from(piece.piece_index);
            let (a, b) = (
                route.path_points.get(index)?.xz(),
                route.path_points.get(index + 1)?.xz(),
            );
            let along = (point - a).dot(b - a) / (b - a).length_squared().max(f32::EPSILON);
            let distance = point.distance(a + (b - a) * along.clamp(0.0, 1.0));
            (distance <= TRACK_DELETE_PICK_RADIUS_M).then_some((entity, distance))
        })
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(entity, _)| entity)
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
