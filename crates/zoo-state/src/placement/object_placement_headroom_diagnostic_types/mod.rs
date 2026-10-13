use avian3d::prelude::RayHitData;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// A single pending result, installed only by runtime verification.
#[derive(Resource, Default)]
pub struct ObjectPlacementHeadroomDiagnosticState {
    pub pending: Option<ObjectPlacementHeadroomObservation>,
}

impl ObjectPlacementHeadroomDiagnosticState {
    pub fn take_pending_observation(&mut self) -> Option<ObjectPlacementHeadroomObservation> {
        self.pending.take()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct ObjectPlacementHeadroomObservation {
    pub preview: Entity,
    pub definition: AssetId,
    pub transform: Transform,
    pub pose: Option<(IVec2, u8)>,
    pub pivot_cm: Option<[i16; 2]>,
    pub clearance_m: f32,
    pub headroom_valid: bool,
    pub flatten_requested: bool,
    pub preview_world: Option<Entity>,
    pub flatten_height_representable: bool,
    pub flattening_world: Option<Entity>,
    pub blocking_ray: Option<ObjectPlacementBlockingHeadroomRay>,
}

#[derive(Clone, Copy, PartialEq)]
pub struct ObjectPlacementBlockingHeadroomRay {
    pub local_cell: IVec2,
    pub world_cell: IVec2,
    pub origin: Vec3,
    pub hit: RayHitData,
    pub exemption: ObjectPlacementTerrainExemptionObservation,
}

/// Scalar lookup results captured during the authoritative ray call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectPlacementTerrainExemptionObservation {
    pub lookup_stage: &'static str,
    pub indexed_entity: Option<Entity>,
    pub indexed_chunk: Option<ObjectPlacementHeadroomChunkObservation>,
    pub cell: Option<UVec2>,
    pub flattened_chunk: Option<Entity>,
    pub hit_marked: Option<bool>,
    pub hit_parent: Option<Entity>,
    pub parent_available: bool,
    pub parent_chunk: Option<ObjectPlacementHeadroomChunkObservation>,
    pub parent_world: Option<Entity>,
    pub parent_matches_indexed: Option<bool>,
}

impl Default for ObjectPlacementTerrainExemptionObservation {
    fn default() -> Self {
        Self {
            lookup_stage: "not reached",
            indexed_entity: None,
            indexed_chunk: None,
            cell: None,
            flattened_chunk: None,
            hit_marked: None,
            hit_parent: None,
            parent_available: false,
            parent_chunk: None,
            parent_world: None,
            parent_matches_indexed: None,
        }
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "shared by placement and verification while remaining a crate-internal observation"
)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectPlacementHeadroomChunkObservation {
    pub coord: IVec2,
    pub origin: Vec2,
    pub spacing_m: f32,
    pub side: u16,
    pub world: Option<Entity>,
}
