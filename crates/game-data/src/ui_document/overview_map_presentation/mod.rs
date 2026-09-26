use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiOverviewCanvas {
    Terrain,
    Water,
    Paths,
    Fences,
    GroundTrack,
    ElevatedPaths,
    SkyTrack,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiOverviewLayer {
    Terrain,
    Water,
    Paths,
    Fences,
    Buildings,
    Animals,
    GroundTrack,
    ElevatedPaths,
    SkyTrack,
    FossilMarkers,
    CameraPosition,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiMapLayerRecord {
    pub kind: UiOverviewLayer,
    pub node: AssetId,
    pub icon: AssetId,
    /// Original in-map marker frame or icon for dynamic layers. This differs
    /// from `icon`, which is the filter-legend toggle sprite.
    pub marker_icon: AssetId,
    pub localization_key: AssetId,
    pub canvas: Option<UiOverviewCanvas>,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiMapColorsRecord {
    pub terrain: [u8; 4],
    pub fence: [u8; 4],
    pub curb: [u8; 4],
    pub zoo_wall: [u8; 4],
    pub path: [u8; 4],
    pub elevated_path: [u8; 4],
    pub ground_track: [u8; 4],
    pub sky_track: [u8; 4],
    pub water: [u8; 4],
}
