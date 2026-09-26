//! Authored guest paths, supports, curbs, and tile-surface definitions.

use crate::AssetId;

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "support piece heights are floating point"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct GuestPathDefinition {
    pub id: AssetId,
    pub object: AssetId,
    pub surface_texture: AssetId,
    pub curb: AssetId,
    pub width_cm: u16,
    pub capacity: u16,
    pub speed_permille: u16,
    pub elevated: bool,
    pub support_prefab: AssetId,
    pub curve_support_prefab: AssetId,
    #[serde(default)]
    pub support_column: Option<super::expanding_columns::ExpandingColumnPresentationDefinition>,
    #[serde(default)]
    pub curve_support_column:
        Option<super::expanding_columns::ExpandingColumnPresentationDefinition>,
    pub max_support_grade_permille: u16,
    pub support_headroom_cm: u16,
    pub surface: GuestPathTileSurfaceProfile,
}
// Five fixed samples in tile-local order: north-west, north-east, south-east,
// south-west, centre.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GuestPathTileSurfaceProfile {
    pub height_cm: [i16; 5],
}
