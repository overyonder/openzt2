use bevy::camera::primitives::Aabb;
use bevy::ecs::system::SystemParam;
use bevy::picking::mesh_picking::ray_cast::{RayCastBackfaces, SimplifiedMesh};
use bevy::prelude::*;

use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::topology::topology_graph_types::{
    FenceEdge, TopologyGrid, TopologyIndex, TopologyNode,
};
use openzt2_zoo_state::transport_tours::transport_circuit_types::{
    CircuitMember, TransportCircuit,
};
use openzt2_zoo_state::transport_tours::transport_topology_types::{
    TrackProfile, TrackSegment, TransportStation, TransportTrackJunction,
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

#[derive(Component, Default)]
pub(super) struct DeleteHoverProbe(Option<Ray3d>);

#[derive(SystemParam)]
pub(super) struct DeleteTargetChanges<'w, 's> {
    topology: Res<'w, TopologyIndex>,
    mesh_assets: Res<'w, Assets<Mesh>>,
    meshes: Query<
        'w,
        's,
        Entity,
        (
            With<Aabb>,
            Or<(With<Mesh3d>, With<Mesh2d>, With<SimplifiedMesh>)>,
            Or<(
                Changed<Aabb>,
                Changed<GlobalTransform>,
                Changed<ChildOf>,
                Changed<ViewVisibility>,
                Added<RayCastBackfaces>,
                AssetChanged<Mesh3d>,
                AssetChanged<Mesh2d>,
                Changed<Mesh3d>,
                Changed<Mesh2d>,
                Changed<SimplifiedMesh>,
            )>,
        ),
    >,
    towers: Query<'w, 's, (), (With<TransportTrackJunction>, Changed<GlobalTransform>)>,
    routes: Query<
        'w,
        's,
        (),
        Or<(
            Changed<TrackSegment>,
            Changed<CircuitMember>,
            Changed<TrackProfile>,
            Changed<TransportTrackConstructionPiece>,
            Added<TransportTrackJunction>,
            Added<TransportStation>,
            Added<TransportCircuit>,
        )>,
    >,
    placeables: Query<'w, 's, (), Changed<PlacedObjectDefinitionReference>>,
    removed_places: RemovedComponents<'w, 's, PlacedObjectDefinitionReference>,
    removed_parents: RemovedComponents<'w, 's, ChildOf>,
    removed_meshes: RemovedComponents<'w, 's, Mesh3d>,
    removed_flat_meshes: RemovedComponents<'w, 's, Mesh2d>,
    removed_simplified_meshes: RemovedComponents<'w, 's, SimplifiedMesh>,
    removed_bounds: RemovedComponents<'w, 's, Aabb>,
    removed_backfaces: RemovedComponents<'w, 's, RayCastBackfaces>,
    removed_route_pieces: RemovedComponents<'w, 's, TransportTrackConstructionPiece>,
    removed_routes: RemovedComponents<'w, 's, TrackSegment>,
    removed_towers: RemovedComponents<'w, 's, TransportTrackJunction>,
    removed_stations: RemovedComponents<'w, 's, TransportStation>,
    removed_circuits: RemovedComponents<'w, 's, TransportCircuit>,
    removed_members: RemovedComponents<'w, 's, CircuitMember>,
}

impl DeleteTargetChanges<'_, '_> {
    fn clear_removals(&mut self) -> bool {
        let counts = [
            self.removed_places.read().count(),
            self.removed_parents.read().count(),
            self.removed_meshes.read().count(),
            self.removed_flat_meshes.read().count(),
            self.removed_simplified_meshes.read().count(),
            self.removed_bounds.read().count(),
            self.removed_backfaces.read().count(),
            self.removed_route_pieces.read().count(),
            self.removed_routes.read().count(),
            self.removed_towers.read().count(),
            self.removed_stations.read().count(),
            self.removed_circuits.read().count(),
            self.removed_members.read().count(),
        ];
        counts.into_iter().any(|count| count != 0)
    }
}

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
    mut cursors: Query<(
        Entity,
        Ref<ConstructionCursor>,
        &mut DeleteHoverProbe,
        Option<&mut DeleteHoverTarget>,
    )>,
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
    mut changes: DeleteTargetChanges,
) {
    let removed = changes.clear_removals();
    let Ok((cursor_entity, cursor, mut previous_probe, previous)) = cursors.single_mut() else {
        return;
    };
    let delete_ray = if *tool == ConstructionTool::Delete {
        ray.0
    } else {
        None
    };
    if !tool.is_changed()
        && delete_ray == previous_probe.0
        && (*tool != ConstructionTool::Delete
            || (!cursor.is_changed()
                && !grid.is_changed()
                && !removed
                && !changes.topology.is_changed()
                && !changes.mesh_assets.is_changed()
                && changes.routes.is_empty()
                && changes.towers.is_empty()
                && changes.placeables.is_empty()
                && !changes
                    .meshes
                    .iter()
                    .any(|entity| placeable_ancestor(entity, &parents, &placeables).is_some())))
    {
        return;
    }
    previous_probe.0 = delete_ray;
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
    match (previous, target) {
        (Some(mut previous), Some(target)) => {
            if previous.0 != target {
                previous.0 = target;
            }
        }
        (None, Some(target)) => {
            commands
                .entity(cursor_entity)
                .insert(DeleteHoverTarget(target));
        }
        (Some(_), None) => {
            commands.entity(cursor_entity).remove::<DeleteHoverTarget>();
        }
        (None, None) => {}
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
    // A fixed 12 m pick height, read the tower's height if tall towers miss.
    towers
        .iter()
        .filter_map(|(entity, transform)| {
            let base = transform.translation();
            let distance = (0_u8..=24)
                .map(|step| base + Vec3::Y * (12.0 * f32::from(step) / 24.0))
                .map(|point| {
                    let along = (point - ray.origin).dot(*ray.direction).max(0.0);
                    point.distance(ray.origin + *ray.direction * along)
                })
                .fold(f32::INFINITY, f32::min);
            (distance <= 2.0).then_some((entity, distance))
        })
        .filter(|(entity, _)| {
            routes.iter().any(|(route, profile)| {
                route.to == *entity && profile.kind == TransportationTrackKind::Sky
            }) && sales.can_sell_track_at(*entity)
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
        .filter(|(entity, _)| sales.can_sell_track_at(*entity))
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(entity, _)| entity)
}

fn placeable_ancestor(
    entity: Entity,
    parents: &Query<&ChildOf>,
    placeables: &Query<(), With<PlacedObjectDefinitionReference>>,
) -> Option<Entity> {
    std::iter::once(entity)
        .chain(parents.iter_ancestors::<ChildOf>(entity))
        .find(|ancestor| placeables.contains(*ancestor))
}
