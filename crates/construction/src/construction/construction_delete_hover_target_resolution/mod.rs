use super::transport_spatial_picking::TransportSpatialPicking;
use bevy::camera::primitives::Aabb;
use bevy::ecs::system::SystemParam;
use bevy::picking::mesh_picking::ray_cast::{RayCastBackfaces, SimplifiedMesh};
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::transport_tours::transport_pick_bounds::TransportPickBounds;

use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;

use super::construction_cursor_terrain_surface_tracking::EffectiveConstructionRay;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::topology::topology_graph_types::{TopologyGrid, TopologyIndex};
use openzt2_zoo_state::transport_tours::transport_circuit_types::{
    CircuitMember, TransportCircuit,
};
use openzt2_zoo_state::transport_tours::transport_topology_types::{
    TrackIncidence, TrackProfile, TrackSegment, TransportStation, TransportTrackJunction,
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

#[derive(SystemParam)]
pub(super) struct DeleteTargetChanges<'w, 's> {
    topology: Query<'w, 's, Ref<'static, TopologyIndex>>,
    meshes: Query<
        'w,
        's,
        Entity,
        (
            With<ConstructionPickableMesh>,
            Or<(
                Changed<ConstructionPickableMesh>,
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
    discarded: MessageReader<'w, 's, ConstructionPickableDiscarded>,
}

#[derive(Message)]
pub(super) struct ConstructionPickableDiscarded;

#[derive(Component, PartialEq)]
pub(super) struct ConstructionPickableMesh(Entity);

#[derive(Message)]
pub(super) struct ConstructionPickableMeshChanged {
    entity: Entity,
    descendants: bool,
}

pub(super) fn notify_added_placeable_pick_mesh(
    inserted: On<Insert<(Mesh3d, Mesh2d, SimplifiedMesh, Aabb)>>,
    parents: Query<&ChildOf>,
    placeables: Query<(), With<PlacedObjectDefinitionReference>>,
    mut changed: MessageWriter<ConstructionPickableMeshChanged>,
) {
    if placeable_ancestor(inserted.entity, &parents, &placeables).is_some() {
        changed.write(ConstructionPickableMeshChanged {
            entity: inserted.entity,
            descendants: false,
        });
    }
}

pub(super) fn notify_changed_placeable_pick_hierarchy(
    inserted: On<Insert<(ChildOf, PlacedObjectDefinitionReference)>>,
    parents: Query<&ChildOf>,
    placeables: Query<(), With<PlacedObjectDefinitionReference>>,
    mut changed: MessageWriter<ConstructionPickableMeshChanged>,
) {
    if placeable_ancestor(inserted.entity, &parents, &placeables).is_some() {
        changed.write(ConstructionPickableMeshChanged {
            entity: inserted.entity,
            descendants: true,
        });
    }
}

pub(super) fn notify_discarded_placeable_pick_mesh(
    discarded: On<Discard<(Mesh3d, Mesh2d, SimplifiedMesh, Aabb, RayCastBackfaces)>>,
    pickable: Query<(), With<ConstructionPickableMesh>>,
    mut changed: MessageWriter<ConstructionPickableMeshChanged>,
    mut invalidated: MessageWriter<ConstructionPickableDiscarded>,
) {
    if pickable.contains(discarded.entity) {
        invalidated.write(ConstructionPickableDiscarded);
        changed.write(ConstructionPickableMeshChanged {
            entity: discarded.entity,
            descendants: false,
        });
    }
}

pub(super) fn notify_discarded_placeable_pick_hierarchy(
    discarded: On<Discard<(ChildOf, PlacedObjectDefinitionReference)>>,
    parents: Query<&ChildOf>,
    placeables: Query<(), With<PlacedObjectDefinitionReference>>,
    mut changed: MessageWriter<ConstructionPickableMeshChanged>,
    mut invalidated: MessageWriter<ConstructionPickableDiscarded>,
) {
    if placeable_ancestor(discarded.entity, &parents, &placeables).is_some() {
        invalidated.write(ConstructionPickableDiscarded);
        changed.write(ConstructionPickableMeshChanged {
            entity: discarded.entity,
            descendants: true,
        });
    }
}

pub(super) fn bind_construction_pickable_meshes(
    mut changes: MessageReader<ConstructionPickableMeshChanged>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    placeables: Query<(), With<PlacedObjectDefinitionReference>>,
    geometry: Query<
        (),
        (
            With<Aabb>,
            Or<(With<Mesh3d>, With<Mesh2d>, With<SimplifiedMesh>)>,
        ),
    >,
    mut meshes: Query<&mut ConstructionPickableMesh>,
    mut commands: Commands,
) {
    let mut dirty = bevy::ecs::entity::EntityHashSet::default();
    for change in changes.read() {
        dirty.insert(change.entity);
        if change.descendants {
            dirty.extend(children.iter_descendants(change.entity));
        }
    }
    for entity in dirty {
        let owner = geometry
            .contains(entity)
            .then(|| placeable_ancestor(entity, &parents, &placeables))
            .flatten();
        match (owner, meshes.get_mut(entity)) {
            (Some(owner), Ok(mut mesh)) => {
                mesh.set_if_neq(ConstructionPickableMesh(owner));
            }
            (Some(owner), Err(_)) => {
                commands
                    .entity(entity)
                    .try_insert(ConstructionPickableMesh(owner));
            }
            (None, Ok(_)) => {
                commands
                    .entity(entity)
                    .try_remove::<ConstructionPickableMesh>();
            }
            (None, Err(_)) => {}
        }
    }
}

pub(super) fn notify_discarded_construction_pickable(
    _: On<
        Discard<(
            PlacedObjectDefinitionReference,
            TransportTrackConstructionPiece,
            TrackSegment,
            TransportTrackJunction,
            TransportStation,
            TransportCircuit,
            CircuitMember,
        )>,
    >,
    mut changed: MessageWriter<ConstructionPickableDiscarded>,
) {
    changed.write(ConstructionPickableDiscarded);
}

/// Walks from the hovered collider to its owning placeable entity, or else
/// takes the fence segment under the pointer, which the delete tool removes
/// as in the original.
#[allow(clippy::too_many_arguments)]
pub(super) fn update_delete_hover_target(
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    mut mesh_ray_cast: MeshRayCast,
    pickable_meshes: Query<&ConstructionPickableMesh>,
    mut cursors: Query<(
        Entity,
        Ref<ConstructionCursor>,
        &WorldMember,
        Ref<EffectiveConstructionRay>,
        Option<&mut DeleteHoverTarget>,
    )>,
    grids: Query<Ref<TopologyGrid>>,
    track_pieces: Query<(Entity, &TransportTrackConstructionPiece)>,
    routes: Query<&TrackSegment>,
    (sales, sky_towers): (
        TransportTrackSaleQueries,
        Query<(Entity, &GlobalTransform), With<TransportTrackJunction>>,
    ),
    (sky_routes, track_incidence, transport_bounds): (
        Query<&TrackProfile, With<TrackSegment>>,
        TrackIncidence,
        TransportPickBounds,
    ),
    (mut changes, spatial): (DeleteTargetChanges, TransportSpatialPicking),
) {
    let removed = !changes.discarded.is_empty();
    changes.discarded.clear();
    let Ok((cursor_entity, cursor, member, ray, previous)) = cursors.single_mut() else {
        return;
    };
    let Ok(grid) = grids.get(member.root) else {
        return;
    };
    if !tool.is_changed()
        && !ray.is_changed()
        && (*tool != ConstructionTool::Delete
            || (ray.0.is_none() && !cursor.is_changed())
            || (!cursor.is_changed()
                && !grid.is_changed()
                && !removed
                && !changes
                    .topology
                    .get(member.root)
                    .is_ok_and(|index| index.is_changed())
                && changes.routes.is_empty()
                && changes.towers.is_empty()
                && changes.placeables.is_empty()
                && changes.meshes.is_empty()))
    {
        return;
    }
    let target = (*tool == ConstructionTool::Delete)
        .then_some(ray.0)
        .flatten()
        .and_then(|ray| {
            // Against meshes, as selection is: trees and plants have no colliders.
            let filter = |entity| pickable_meshes.contains(entity);
            mesh_ray_cast
                .cast_ray(ray, &MeshRayCastSettings::default().with_filter(&filter))
                .first()
                .and_then(|(hit, _)| pickable_meshes.get(*hit).ok().map(|mesh| mesh.0))
        })
        .or_else(|| {
            (*tool == ConstructionTool::Delete && cursor.over_terrain)
                .then(|| {
                    let topology = changes.topology.get(member.root).ok()?;
                    nearest_fence(cursor.world.xz(), &topology, *grid)
                })
                .flatten()
        })
        .or_else(|| {
            (*tool == ConstructionTool::Delete)
                .then_some(ray.0)
                .flatten()
                .and_then(|ray| {
                    let mut candidates = bevy::ecs::entity::EntityHashSet::default();
                    spatial.extend_along_ray(member.root, ray, 3.0, &mut candidates);
                    nearest_sky_tower(
                        ray,
                        candidates,
                        &sky_towers,
                        &sky_routes,
                        &track_incidence,
                        &transport_bounds,
                        &sales,
                    )
                })
        })
        .or_else(|| {
            (*tool == ConstructionTool::Delete && cursor.over_terrain)
                .then(|| {
                    nearest_track_piece(
                        cursor.world.xz(),
                        spatial.nearby(member.root, cursor.world.xz(), 8.0),
                        &track_pieces,
                        &routes,
                        &sales,
                    )
                })
                .flatten()
        });
    match (previous, target) {
        (Some(mut previous), Some(target)) => {
            previous.set_if_neq(DeleteHoverTarget(target));
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

fn nearest_fence(point: Vec2, topology: &TopologyIndex, grid: TopologyGrid) -> Option<Entity> {
    if !point.is_finite() || !grid.spacing_m.is_finite() || grid.spacing_m <= 0.0 {
        return None;
    }
    let relative = point - grid.origin;
    let radius = Vec2::splat(FENCE_DELETE_PICK_RADIUS_M);
    let min = ((relative - radius) / grid.spacing_m).floor().as_ivec2();
    let max = ((relative + radius) / grid.spacing_m).ceil().as_ivec2();
    topology
        .fences_within(min, max)
        .filter_map(|entity| {
            let edge = topology.fence_key(entity)?;
            let a = grid.cell_translation(edge.lo).xz();
            let b = grid.cell_translation(edge.hi).xz();
            let along = (point - a).dot(b - a) / (b - a).length_squared().max(f32::EPSILON);
            let distance = point.distance(a + (b - a) * along.clamp(0.0, 1.0));
            (distance <= FENCE_DELETE_PICK_RADIUS_M).then_some((entity, distance))
        })
        .min_by(|(left_entity, left), (right_entity, right)| {
            left.total_cmp(right)
                .then_with(|| left_entity.cmp(right_entity))
        })
        .map(|(entity, _)| entity)
}

/// The sky tower the pointer ray passes within 2 m of, along its height,
/// whose span the sale can remove.
fn nearest_sky_tower(
    ray: Ray3d,
    candidates: bevy::ecs::entity::EntityHashSet,
    towers: &Query<(Entity, &GlobalTransform), With<TransportTrackJunction>>,
    routes: &Query<&TrackProfile, With<TrackSegment>>,
    incidence: &TrackIncidence,
    bounds: &TransportPickBounds,
    sales: &TransportTrackSaleQueries,
) -> Option<Entity> {
    towers
        .iter_many(candidates)
        .filter_map(Result::ok)
        .filter_map(|(entity, _)| {
            let distance = bounds.hit_distance(entity, ray)?;
            incidence
                .incoming(entity)
                .any(|route| {
                    routes
                        .get(route)
                        .is_ok_and(|profile| profile.kind == TransportationTrackKind::Sky)
                })
                .then_some((entity, distance))
        })
        .filter(|(entity, _)| sales.can_sell_track_at(*entity))
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(entity, _)| entity)
}

fn nearest_track_piece(
    point: Vec2,
    candidates: bevy::ecs::entity::EntityHashSet,
    pieces: &Query<(Entity, &TransportTrackConstructionPiece)>,
    routes: &Query<&TrackSegment>,
    sales: &TransportTrackSaleQueries,
) -> Option<Entity> {
    pieces
        .iter_many(candidates)
        .filter_map(Result::ok)
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
