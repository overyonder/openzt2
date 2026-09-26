use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Last terrain revision applied to the water meshes.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TerrainWaterRevision(pub u64);

/// One material-homogeneous Bevy water mesh parented to its terrain chunk.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TerrainWaterSurface;

/// A vertical water strip between two different surface heights.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TerrainWaterfallSurface;

/// A connected water region from the terrain data. Its transform places the
/// spatial-audio emitter at the region's centroid.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct NaturalWaterRegion {
    /// Stable identity of the loaded water style referenced by this region.
    pub style: AssetId,
    pub area_m2: f32,
}

/// Marks a chunk after spawning the water regions that begin in it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct TerrainWaterRegionsProjected;
