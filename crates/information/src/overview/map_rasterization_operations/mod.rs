use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{
    ui_document::{overview_map_presentation::UiOverviewCanvas, widget::UiWidgetRecord},
    world_definitions::transportation_and_tours::TransportationTrackKind,
};

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::information::overview::overview_types::OverviewMapCanvas;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId};
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_zoo_state::{
    terrain::{terrain_chunk_types::TerrainChunk, terrain_edit_types::TerrainChanged},
    topology::topology_graph_types::{FenceEdge, FenceEnd, FenceSegment, FenceStart, PathTile, TopologyNode},
    transport_tours::transport_topology_types::{TrackEnd, TrackRoute, TrackSegment, TrackStart},
};

use super::topology_rasterization_operations::{
    rasterize_fence_edges_into_overview_map_pixels, rasterize_path_tiles_into_overview_map_pixels,
    rasterize_transport_track_segments_into_overview_map_pixels,
};

pub(super) const OVERVIEW_MAP_IMAGE_EXTENT: u32 = 512;

#[derive(Component)]
pub(crate) struct OverviewTerrainWaterRaster;

#[derive(Component, Default)]
pub(crate) struct PendingOverviewMapPixels(pub(super) Option<Vec<u8>>);

pub(super) const fn is_terrain_or_water(source: UiOverviewCanvas) -> bool {
    matches!(source, UiOverviewCanvas::Terrain | UiOverviewCanvas::Water)
}

pub(super) fn blank_overview_map_pixels() -> Vec<u8> {
    vec![0; OVERVIEW_MAP_IMAGE_EXTENT as usize * OVERVIEW_MAP_IMAGE_EXTENT as usize * 4]
}

#[derive(SystemParam)]
pub(crate) struct OverviewMapTopologyChanges<'w, 's> {
    fences: Query<'w, 's, (), Or<(Changed<FenceEdge>, Changed<FenceStart>, Changed<FenceEnd>)>>,
    paths: Query<
        'w,
        's,
        (),
        (
            With<PathTile>,
            Or<(Changed<PathTile>, Changed<GlobalTransform>)>,
        ),
    >,
    tracks: Query<
        'w,
        's,
        (),
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
    mut canvases: Query<&mut OverviewMapCanvas>,
) {
    let edited_terrain = terrain.read().count() != 0;
    let removed_chunks = removed_chunks.read().count() != 0;
    let fences_removed = topology.fences_removed.read().count() != 0;
    let paths_removed = topology.paths_removed.read().count() != 0;
    let tracks_removed = topology.tracks_removed.read().count() != 0;
    let all = definitions.is_changed() || definition_assets.is_changed() || !bounds.is_empty();
    let terrain_changed =
        edited_terrain || removed_chunks || !chunks.is_empty() || terrain_assets.is_changed();
    let fences_changed =
        fences_removed || !topology.fences.is_empty() || !topology.nodes.is_empty();
    let paths_changed = paths_removed || !topology.paths.is_empty();
    let tracks_changed =
        tracks_removed || !topology.tracks.is_empty() || !topology.nodes.is_empty();
    for mut canvas in &mut canvases {
        let changed = all
            || match canvas.source {
                UiOverviewCanvas::Terrain | UiOverviewCanvas::Water => terrain_changed,
                UiOverviewCanvas::Paths | UiOverviewCanvas::ElevatedPaths => paths_changed,
                UiOverviewCanvas::Fences => fences_changed,
                UiOverviewCanvas::GroundTrack | UiOverviewCanvas::SkyTrack => tracks_changed,
            };
        if changed && canvas.painted {
            canvas.painted = false;
        }
    }
}

pub(crate) fn refresh_overview_map_canvas_policies(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    surfaces: Query<(&UiNodeId, &UiDocumentOwner)>,
    mut canvases: Query<(Entity, &mut OverviewMapCanvas), With<PendingOverviewMapPixels>>,
    mut commands: Commands,
) {
    if !documents.is_changed() {
        return;
    }
    for (entity, mut canvas) in &mut canvases {
        let Some(policy) = (|| {
            let (node, owner) = surfaces.get(canvas.surface).ok()?;
            let root = roots.get(owner.0).ok()?;
            let document = documents.get(&root.document)?;
            let UiWidgetRecord::WorldMap { layers, colors } = &document
                .canonical_ui_document()
                .nodes
                .get(node.index as usize)?
                .widget
            else {
                return None;
            };
            Some(
                layers
                    .get(canvas.layer as usize)
                    .and_then(|layer| layer.canvas)
                    .map(|source| (source, *colors)),
            )
        })() else {
            continue;
        };
        let Some((source, palette)) = policy else {
            commands.entity(entity).try_despawn();
            continue;
        };
        if canvas.source != source || canvas.palette != palette {
            if is_terrain_or_water(canvas.source) != is_terrain_or_water(source) {
                if is_terrain_or_water(source) {
                    commands.entity(entity).insert(OverviewTerrainWaterRaster);
                } else {
                    commands
                        .entity(entity)
                        .remove::<OverviewTerrainWaterRaster>();
                }
            }
            canvas.source = source;
            canvas.palette = palette;
            canvas.painted = false;
        }
    }
}

pub(crate) fn rasterize_dirty_overview_topology_canvases(
    mut canvases: Query<
        (&mut OverviewMapCanvas, &mut PendingOverviewMapPixels),
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
    for (mut canvas, mut output) in &mut canvases {
        if canvas.painted {
            continue;
        }
        let mut pixels = blank_overview_map_pixels();
        match canvas.source {
            UiOverviewCanvas::Fences => rasterize_fence_edges_into_overview_map_pixels(
                &mut pixels,
                bounds,
                &canvas.palette,
                definitions,
                &fence_edges,
                &nodes,
            ),
            UiOverviewCanvas::Paths | UiOverviewCanvas::ElevatedPaths => {
                rasterize_path_tiles_into_overview_map_pixels(
                    &mut pixels,
                    bounds,
                    &canvas.palette,
                    definitions,
                    &path_tiles,
                    canvas.source == UiOverviewCanvas::ElevatedPaths,
                )
            }
            UiOverviewCanvas::GroundTrack | UiOverviewCanvas::SkyTrack => {
                let (color, kind) = if canvas.source == UiOverviewCanvas::GroundTrack {
                    (canvas.palette.ground_track, TransportationTrackKind::Ground)
                } else {
                    (canvas.palette.sky_track, TransportationTrackKind::Sky)
                };
                rasterize_transport_track_segments_into_overview_map_pixels(
                    &mut pixels,
                    bounds,
                    color,
                    kind,
                    &tracks,
                    &nodes,
                    definitions,
                );
            }
            UiOverviewCanvas::Terrain | UiOverviewCanvas::Water => continue,
        }
        output.0 = Some(pixels);
        canvas.painted = true;
    }
}

pub(crate) fn publish_pending_overview_map_images(
    mut ready: Query<(&ImageNode, &mut PendingOverviewMapPixels)>,
    mut images: ResMut<Assets<Image>>,
) {
    for (node, mut pixels) in &mut ready {
        if pixels.0.is_some() {
            if let Some(image) = images.get_mut(&node.image) {
                image.into_inner().data = pixels.0.take();
            }
        }
    }
}
