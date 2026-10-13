use avian3d::prelude::Collider;
use bevy::{ecs::system::SystemParam, prelude::*};

use crate::plugins::{
    camera::world_pointer_ray_types::WorldPointerRay,
    construction::construction_interaction_types::{ConstructionCursor, ConstructionPreview},
    input::input_types::PrimaryPointerInputState,
    placement::{
        object_placement_headroom_diagnostic_types::{
            ObjectPlacementBlockingHeadroomRay, ObjectPlacementHeadroomDiagnosticState,
            ObjectPlacementHeadroomObservation,
        },
        placed_object_types::PlacedObjectDefinitionReference,
    },
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    world_spawn::{
        selected_world_identity::SelectedWorldIdentity,
        world_membership_types::{WorldMember, WorldRoot},
    },
};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

#[derive(SystemParam)]
pub(super) struct VerificationHeadroomOwnerQueries<'w, 's> {
    pointer: Res<'w, PrimaryPointerInputState>,
    pointer_ray: Res<'w, WorldPointerRay>,
    cursors: Query<'w, 's, &'static ConstructionCursor>,
    worlds: Query<'w, 's, &'static SelectedWorldIdentity, With<WorldRoot>>,
    terrain_index: Res<'w, TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    owners: Query<
        'w,
        's,
        (
            Option<&'static ChildOf>,
            Has<Collider>,
            Has<TerrainChunk>,
            Option<&'static PlacedObjectDefinitionReference>,
            Has<ConstructionPreview>,
            Option<&'static WorldMember>,
        ),
    >,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct VerificationHeadroomLogKey {
    observation: ObjectPlacementHeadroomObservation,
    pointer_screen: Vec2,
    pointer_available: bool,
    pointer_ray: Option<Ray3d>,
    cursor_hit: Option<ConstructionCursor>,
}

pub(super) fn log_changed_object_placement_headroom_result(
    mut state: ResMut<ObjectPlacementHeadroomDiagnosticState>,
    facts: VerificationHeadroomOwnerQueries,
    mut previous: Local<Option<VerificationHeadroomLogKey>>,
) {
    let Some(observation) = state.take_pending_observation() else {
        return;
    };
    let cursor_hit = facts
        .cursors
        .single()
        .ok()
        .filter(|cursor| cursor.over_terrain)
        .copied();
    let current = VerificationHeadroomLogKey {
        observation,
        pointer_screen: facts.pointer.screen,
        pointer_available: facts.pointer.available,
        pointer_ray: facts.pointer_ray.0,
        cursor_hit,
    };
    if previous.replace(current) == Some(current) {
        return;
    }
    let world = facts
        .owners
        .get(observation.preview)
        .ok()
        .and_then(|(_, _, _, _, _, member)| member)
        .and_then(|member| facts.worlds.get(member.root).ok());
    info!(target: "openzt2_verification",
            preview = ?observation.preview, definition = ?observation.definition,
            map = ?world.map(|world| world.map), start = ?world.map(|world| world.start),
            pointer_available = facts.pointer.available, pointer_screen = ?facts.pointer.screen,
            pointer_ray = ?facts.pointer_ray.0, cursor_hit = ?cursor_hit,
            snapped_transform = ?observation.transform, footprint_pose = ?observation.pose,
            pivot_cm = ?observation.pivot_cm, clearance_m = observation.clearance_m,
            headroom_valid = observation.headroom_valid,
            flatten_requested = observation.flatten_requested,
            preview_world = ?observation.preview_world,
            flatten_height_representable = observation.flatten_height_representable,
            flattening_world = ?observation.flattening_world,
            blocking_hit = ?observation.blocking_ray.map(|ray| ray.hit),
            "authoritative object placement headroom result");
    let Some(ray) = observation.blocking_ray else {
        return;
    };
    log_first_blocking_headroom_ray(&facts, observation, ray);
}

fn log_first_blocking_headroom_ray(
    facts: &VerificationHeadroomOwnerQueries,
    observation: ObjectPlacementHeadroomObservation,
    ray: ObjectPlacementBlockingHeadroomRay,
) {
    let terrain = terrain_chunk_at(&facts.terrain_index, ray.world_cell.as_vec2())
        .and_then(|entity| facts.chunks.get(entity).ok())
        .and_then(|(chunk, edited)| {
            facts
                .terrain_assets
                .get(&chunk.asset)
                .and_then(|asset| sample_terrain(chunk, asset, edited, ray.world_cell.as_vec2()))
        });
    // Include the hit and at most seven parents; unknown owners stay explicit.
    let mut ancestry = [None; 8];
    let mut next = Some(ray.hit.entity);
    let mut ancestry_end = "depth_limit";
    let mut hit_has_collider = None;
    let mut terrain_chunk_owner = None;
    let mut placed_object_owner = None;
    let mut preview_owner = None;
    let mut collider_world_root = None;
    for slot in &mut ancestry {
        let Some(entity) = next else {
            ancestry_end = "no_parent";
            break;
        };
        *slot = Some(entity);
        let Ok((parent, collider, chunk, placed, preview, member)) = facts.owners.get(entity)
        else {
            ancestry_end = "entity_unavailable";
            break;
        };
        if entity == ray.hit.entity {
            hit_has_collider = Some(collider);
        }
        if chunk && terrain_chunk_owner.is_none() {
            terrain_chunk_owner = Some(entity);
        }
        if let Some(placed) = placed.filter(|_| placed_object_owner.is_none()) {
            placed_object_owner = Some((entity, placed.definition));
        }
        if preview && preview_owner.is_none() {
            preview_owner = Some(entity);
        }
        if let Some(member) = member.filter(|_| collider_world_root.is_none()) {
            collider_world_root = Some(member.root);
        }
        next = parent.map(ChildOf::parent);
        if next.is_none() {
            ancestry_end = "no_parent";
            break;
        }
    }
    info!(target: "openzt2_verification",
            preview = ?observation.preview, definition = ?observation.definition,
            local_cell = ?ray.local_cell, world_cell = ?ray.world_cell,
            ray_origin = ?ray.origin, ray_direction = ?Dir3::Y,
            clearance_m = observation.clearance_m, solid = true,
            hit = ?ray.hit, hit_has_collider = ?hit_has_collider,
            authoritative_exemption = ?ray.exemption,
            ownership_sampling = "exemption captured in ray system; ancestry, pointer and terrain sample are later logger context",
            ancestry = ?ancestry, ancestry_end,
            terrain_chunk_owner = ?terrain_chunk_owner,
            placed_object_owner = ?placed_object_owner, preview_owner = ?preview_owner,
            collider_world_root = ?collider_world_root,
            canonical_terrain_height_m = ?terrain.map(|point| point.height_m),
            canonical_terrain_normal = ?terrain.map(|point| point.normal),
            "first blocking authoritative headroom ray");
}
