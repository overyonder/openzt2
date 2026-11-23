use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{
    ui_document::overview_map_presentation::UiOverviewCanvas,
    world_definitions::transportation_and_tours::TransportationTrackKind,
};

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_zoo_state::{
    terrain::{terrain_chunk_types::TerrainChunk, terrain_edit_types::TerrainChanged},
    topology::topology_graph_types::{FenceEdge, FenceEnd, FenceSegment, FenceStart, PathTile, TopologyNode},
    transport_tours::transport_topology_types::{TrackEnd, TrackRoute, TrackSegment, TrackStart},
};

use super::canvas_hydration_operations::OverviewCanvasPolicy;
use super::topology_rasterization_operations::{
    OverviewTopologyRaster, rasterize_fence_edges_into_overview_map_pixels,
    rasterize_path_tiles_into_overview_map_pixels,
    rasterize_transport_track_segments_into_overview_map_pixels,
};

pub(super) const OVERVIEW_MAP_IMAGE_EXTENT: u32 = 512;

#[derive(Component)]
pub(crate) struct OverviewTerrainWaterRaster;

#[derive(Component, Default)]
pub(crate) struct PendingOverviewMapPixels {
    pub(super) pixels: Option<Vec<u8>>,
    pub(super) painted: bool,
}

pub(super) const fn is_terrain_or_water(source: UiOverviewCanvas) -> bool {
    matches!(source, UiOverviewCanvas::Terrain | UiOverviewCanvas::Water)
}

pub(super) fn blank_overview_map_pixels() -> Vec<u8> {
    vec![0; OVERVIEW_MAP_IMAGE_EXTENT as usize * OVERVIEW_MAP_IMAGE_EXTENT as usize * 4]
}

#[derive(SystemParam)]
pub(crate) struct OverviewMapTopologyChanges<'w, 's> {
    fences: Query<'w, 's, (), Or<(Changed<FenceEdge>, Changed<FenceStart>, Changed<FenceEnd>)>>,
    paths: Query<'w, 's, Ref<'static, PathTile>, Or<(Changed<PathTile>, Changed<GlobalTransform>)>>,
    tracks: Query<
        'w,
        's,
        Ref<'static, TrackSegment>,
        Or<(
            Changed<TrackSegment>,
            Changed<TrackStart>,
            Changed<TrackEnd>,
        )>,
    >,
    nodes: Query<'w, 's, (), (With<TopologyNode>, Changed<GlobalTransform>)>,
    fences_removed: RemovedComponents<'w, 's, FenceEdge>,
    paths_removed: RemovedComponents<'w, 's, PathTile>,
    tracks_removed: RemovedComponents<'w, 's, TrackSegment>,
    track_starts_removed: RemovedComponents<'w, 's, TrackStart>,
    track_ends_removed: RemovedComponents<'w, 's, TrackEnd>,
}

pub(crate) fn invalidate_changed_overview_map_sources(
    mut terrain: MessageReader<TerrainChanged>,
    mut topology: OverviewMapTopologyChanges,
    terrain_assets: Res<Assets<TerrainAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    bounds: Query<(), Changed<WorldTerrainHorizontalBounds>>,
    chunks: Query<(), Changed<TerrainChunk>>,
    mut removed_chunks: RemovedComponents<TerrainChunk>,
    mut canvases: Query<(Ref<OverviewCanvasPolicy>, &mut PendingOverviewMapPixels)>,
) {
    let edited_terrain = terrain.read().count() != 0;
    let removed_chunks = removed_chunks.read().count() != 0;
    let fences_removed = topology.fences_removed.read().count() != 0;
    let paths_removed = topology.paths_removed.read().count() != 0;
    let tracks_removed = topology.tracks_removed.read().count()
        + topology.track_starts_removed.read().count()
        + topology.track_ends_removed.read().count()
        != 0;
    let all = definitions.is_changed() || definition_assets.is_changed() || !bounds.is_empty();
    let terrain_changed =
        edited_terrain || removed_chunks || !chunks.is_empty() || terrain_assets.is_changed();
    let fences_changed =
        fences_removed || !topology.fences.is_empty() || !topology.nodes.is_empty();
    let mut path_kinds = [paths_removed; 2];
    let mut track_kinds = [tracks_removed || !topology.nodes.is_empty(); 2];
    if !all {
        let view = definitions.get(&definition_assets);
        for path in &topology.paths {
            let policy = view.and_then(|view| view.find_path(path.definition));
            if path.is_changed() && !path.is_added() || policy.is_none() {
                path_kinds = [true; 2];
                break;
            }
            if let Some(policy) = policy {
                path_kinds[usize::from(policy.elevated)] = true;
            }
        }
        for track in &topology.tracks {
            let policy = view.and_then(|view| view.find_track(track.definition));
            if track.is_changed() && !track.is_added() || policy.is_none() {
                track_kinds = [true; 2];
                break;
            }
            if let Some(policy) = policy {
                track_kinds[usize::from(policy.kind == TransportationTrackKind::Sky)] = true;
            }
        }
    }
    for (canvas, mut paint) in &mut canvases {
        let changed = canvas.is_changed()
            || all
            || match canvas.source {
                UiOverviewCanvas::Terrain | UiOverviewCanvas::Water => terrain_changed,
                UiOverviewCanvas::Paths => path_kinds[0],
                UiOverviewCanvas::ElevatedPaths => path_kinds[1],
                UiOverviewCanvas::Fences => fences_changed,
                UiOverviewCanvas::GroundTrack => track_kinds[0],
                UiOverviewCanvas::SkyTrack => track_kinds[1],
            };
        if changed && paint.painted {
            paint.painted = false;
        }
    }
}

pub(crate) fn rasterize_dirty_overview_topology_canvases(
    mut canvases: Query<
        (Entity, &OverviewCanvasPolicy, &mut PendingOverviewMapPixels),
        Without<OverviewTerrainWaterRaster>,
    >,
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    fence_edges: Query<FenceSegment>,
    path_tiles: Query<(&PathTile, &GlobalTransform)>,
    tracks: Query<TrackRoute>,
    nodes: Query<&GlobalTransform>,
) {
    let (Ok(bounds), Some(definitions)) = (
        world_bounds.single(),
        active_world_definitions.get(&world_definition_assets),
    ) else {
        return;
    };
    let size = bounds.max - bounds.min;
    if !size.is_finite() || size.min_element() <= 0.0 {
        return;
    }
    let mut outputs = Vec::<OverviewTopologyRaster>::new();
    for (entity, canvas, paint) in &canvases {
        if paint.painted {
            continue;
        }
        if let Some(output) = outputs
            .iter_mut()
            .find(|output| output.source == canvas.source && output.palette == canvas.palette)
        {
            output.canvases.push(entity);
        } else {
            outputs.push(OverviewTopologyRaster {
                source: canvas.source,
                palette: canvas.palette,
                canvases: vec![entity],
                pixels: blank_overview_map_pixels(),
            });
        }
    }
    if outputs.is_empty() {
        return;
    }
    rasterize_fence_edges_into_overview_map_pixels(
        &mut outputs,
        bounds,
        definitions,
        &fence_edges,
        &nodes,
    );
    rasterize_path_tiles_into_overview_map_pixels(&mut outputs, bounds, definitions, &path_tiles);
    rasterize_transport_track_segments_into_overview_map_pixels(
        &mut outputs,
        bounds,
        definitions,
        &tracks,
        &nodes,
    );
    for output in outputs {
        let mut subscribers = output.canvases.into_iter().peekable();
        let mut pixels = Some(output.pixels);
        while let Some(entity) = subscribers.next() {
            if let Ok((_, _, mut paint)) = canvases.get_mut(entity) {
                paint.pixels = if subscribers.peek().is_some() {
                    pixels.clone()
                } else {
                    pixels.take()
                };
                paint.painted = true;
            }
        }
    }
}

pub(crate) fn publish_pending_overview_map_images(
    mut ready: Query<(&ImageNode, &mut PendingOverviewMapPixels, &mut Visibility)>,
    mut images: ResMut<Assets<Image>>,
) {
    for (node, mut paint, mut visibility) in &mut ready {
        if paint.pixels.is_some() {
            if let Some(image) = images.get_mut(&node.image) {
                image.into_inner().data = paint.pixels.take();
                visibility.set_if_neq(Visibility::Inherited);
            }
        }
    }
}
