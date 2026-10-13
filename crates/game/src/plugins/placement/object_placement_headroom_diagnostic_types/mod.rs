use avian3d::prelude::RayHitData;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// A single pending result, installed only by runtime verification.
#[derive(Resource, Default)]
pub(crate) struct ObjectPlacementHeadroomDiagnosticState {
    pub(super) pending: Option<ObjectPlacementHeadroomObservation>,
}

impl ObjectPlacementHeadroomDiagnosticState {
    pub(crate) fn take_pending_observation(
        &mut self,
    ) -> Option<ObjectPlacementHeadroomObservation> {
        self.pending.take()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ObjectPlacementHeadroomObservation {
    pub(crate) preview: Entity,
    pub(crate) definition: AssetId,
    pub(crate) transform: Transform,
    pub(crate) pose: Option<(IVec2, u8)>,
    pub(crate) pivot_cm: Option<[i16; 2]>,
    pub(crate) clearance_m: f32,
    pub(crate) headroom_valid: bool,
    pub(crate) flatten_requested: bool,
    pub(crate) preview_world: Option<Entity>,
    pub(crate) flatten_height_representable: bool,
    pub(crate) flattening_world: Option<Entity>,
    pub(crate) blocking_ray: Option<ObjectPlacementBlockingHeadroomRay>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ObjectPlacementBlockingHeadroomRay {
    pub(crate) local_cell: IVec2,
    pub(crate) world_cell: IVec2,
    pub(crate) origin: Vec3,
    pub(crate) hit: RayHitData,
    pub(crate) exemption: ObjectPlacementTerrainExemptionObservation,
}

/// Scalar lookup results captured during the authoritative ray call.
#[allow(
    clippy::redundant_pub_crate,
    reason = "shared by placement and verification while remaining a crate-internal observation"
)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ObjectPlacementTerrainExemptionObservation {
    pub(crate) lookup_stage: &'static str,
    pub(crate) indexed_entity: Option<Entity>,
    pub(crate) indexed_chunk: Option<ObjectPlacementHeadroomChunkObservation>,
    pub(crate) cell: Option<UVec2>,
    pub(crate) flattened_chunk: Option<Entity>,
    pub(crate) hit_marked: Option<bool>,
    pub(crate) hit_parent: Option<Entity>,
    pub(crate) parent_available: bool,
    pub(crate) parent_chunk: Option<ObjectPlacementHeadroomChunkObservation>,
    pub(crate) parent_world: Option<Entity>,
    pub(crate) parent_matches_indexed: Option<bool>,
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
pub(crate) struct ObjectPlacementHeadroomChunkObservation {
    pub(crate) coord: IVec2,
    pub(crate) origin: Vec2,
    pub(crate) spacing_m: f32,
    pub(crate) side: u16,
    pub(crate) world: Option<Entity>,
}
