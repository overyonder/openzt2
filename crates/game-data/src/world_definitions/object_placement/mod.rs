//! Authored object footprints, entrances, and placement constraints.

use crate::AssetId;

mod automatic_placement_bounds;
mod object_placement_flag_operations;

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct PlacementConstraints(u32);

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct FootprintCellFlags(u16);

#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct PlaceableDefinition {
    pub id: AssetId,
    /// Authored `ZTPlacementData.weight`. The placement cursor divides its
    /// base interpolation rate by positive values of this field.
    pub weight: f32,
    pub footprint: Vec<FootprintCell>,
    pub pivot_cm: [i16; 2],
    pub diagonal_footprint: Vec<FootprintCell>,
    pub diagonal_pivot_cm: [i16; 2],
    pub rotation_increment_degrees: u16,
    pub constraints: PlacementConstraints,
    pub max_slope_permille: u16,
    pub minimum_headroom_metres: f32,
    pub apply_height_modifier: bool,
    pub price_cents: i64,
    pub unlock: AssetId,
    pub entrances: Vec<EntranceDefinition>,
    pub moving_footprint: bool,
    /// The authored `ZTPlacementData autoFootprint` rule derives the cell
    /// footprint from the scene-prefab collision/model bounds.
    pub automatic_footprint: bool,
    /// Grid-snapped objects whose authored `stompData` prevents the `path`
    /// entity type collide with ground path tiles under their footprint.
    #[serde(default)]
    pub ground_paths_block_placement: bool,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FootprintCell {
    pub offset: [i16; 2],
    pub flags: FootprintCellFlags,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum EntrancePurpose {
    Guest,
    Staff,
    Service,
    Vehicle,
    Animal,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct EntranceDefinition {
    pub position_cm: [i16; 3],
    pub forward_snorm: [i16; 3],
    pub purpose: EntrancePurpose,
}
