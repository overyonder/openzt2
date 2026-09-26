use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    overview_map_presentation::{UiMapColorsRecord, UiOverviewCanvas},
    widget::UiWidgetRecord,
};
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;

use super::{
    overview_types::OverviewMapCanvas,
    terrain_rasterization_operations::rasterize_terrain_or_water_into_overview_map_pixels,
    topology_rasterization_operations::{
        rasterize_fence_edges_into_overview_map_pixels,
        rasterize_path_tiles_into_overview_map_pixels,
        rasterize_transport_track_segments_into_overview_map_pixels,
    },
};

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_edit_types::TerrainChanged;
use crate::plugins::topology::topology_edit_types::TopologyChanged;
use crate::plugins::topology::topology_graph_types::FenceEdge;
use crate::plugins::topology::topology_graph_types::PathTile;
use crate::plugins::transport_tours::transport_topology_types::TrackSegment;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_node_projection_components::UiNodeId;
use crate::plugins::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

#[allow(clippy::too_many_arguments)]
pub(in crate::plugins::information) fn rebuild_changed_overview_map_canvas_images_from_world_state(
    mut terrain_change_messages: MessageReader<TerrainChanged>,
    mut topology_change_messages: MessageReader<TopologyChanged>,
    mut overview_map_canvases: Query<(&mut OverviewMapCanvas, &ImageNode)>,
    overview_map_surfaces: Query<(&UiNodeId, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    fence_edges: Query<&FenceEdge>,
    path_tiles: Query<(&PathTile, &GlobalTransform)>,
    transport_track_segments: Query<&TrackSegment>,
    topology_nodes: Query<&GlobalTransform>,
    mut overview_map_images: ResMut<Assets<Image>>,
) {
    let terrain_changed = terrain_change_messages.read().count() > 0;
    let topology_changed = topology_change_messages.read().count() > 0;
    let overview_map_source_changed = terrain_changed || topology_changed;
    let Ok(world_bounds) = world_bounds.single() else {
        return;
    };
    let world_bounds_size = world_bounds.max - world_bounds.min;
    if !world_bounds_size.is_finite() || world_bounds_size.min_element() <= 0.0 {
        return;
    }
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    if terrain_chunks.is_empty()
        || terrain_chunks
            .iter()
            .any(|(terrain_chunk, _)| terrain_assets.get(&terrain_chunk.asset).is_none())
    {
        return;
    }
    for (mut overview_map_canvas, image_node) in &mut overview_map_canvases {
        if overview_map_canvas.painted && !overview_map_source_changed {
            continue;
        }
        let Ok((surface_node, document_owner)) =
            overview_map_surfaces.get(overview_map_canvas.surface)
        else {
            continue;
        };
        let Ok(document_root) = ui_document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(ui_document) = ui_documents.get(&document_root.document) else {
            continue;
        };
        let Some(UiWidgetRecord::WorldMap { layers, colors }) = ui_document
            .canonical_ui_document()
            .nodes
            .get(surface_node.index as usize)
            .map(|record| &record.widget)
        else {
            continue;
        };
        let Some(layer_record) = layers.get(overview_map_canvas.layer as usize) else {
            continue;
        };
        let Some(authored_canvas) = layer_record.canvas.as_ref() else {
            continue;
        };
        let Some(mut overview_map_image) = overview_map_images.get_mut(&image_node.image) else {
            continue;
        };
        let Some(overview_map_pixels) = overview_map_image.data.as_mut() else {
            continue;
        };
        overview_map_pixels.fill(0);
        rasterize_authored_overview_map_layer_from_world_state(
            overview_map_pixels,
            world_bounds,
            authored_canvas,
            colors,
            &terrain_chunks,
            &terrain_assets,
            world_definitions,
            &fence_edges,
            &path_tiles,
            &transport_track_segments,
            &topology_nodes,
        );
        overview_map_canvas.painted = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn rasterize_authored_overview_map_layer_from_world_state(
    overview_map_pixels: &mut [u8],
    world_bounds: &WorldTerrainHorizontalBounds,
    authored_canvas: &UiOverviewCanvas,
    rasterization_palette: &UiMapColorsRecord,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_assets: &Assets<TerrainAsset>,
    world_definitions: WorldDefinitionsView<'_>,
    fence_edges: &Query<&FenceEdge>,
    path_tiles: &Query<(&PathTile, &GlobalTransform)>,
    transport_track_segments: &Query<&TrackSegment>,
    topology_node_transforms: &Query<&GlobalTransform>,
) {
    match authored_canvas {
        UiOverviewCanvas::Terrain => rasterize_terrain_or_water_into_overview_map_pixels(
            overview_map_pixels,
            world_bounds,
            terrain_chunks,
            terrain_assets,
            world_definitions,
            false,
            rasterization_palette,
        ),
        UiOverviewCanvas::Water => rasterize_terrain_or_water_into_overview_map_pixels(
            overview_map_pixels,
            world_bounds,
            terrain_chunks,
            terrain_assets,
            world_definitions,
            true,
            rasterization_palette,
        ),
        UiOverviewCanvas::Fences => rasterize_fence_edges_into_overview_map_pixels(
            overview_map_pixels,
            world_bounds,
            rasterization_palette,
            world_definitions,
            fence_edges,
            topology_node_transforms,
        ),
        UiOverviewCanvas::Paths => rasterize_path_tiles_into_overview_map_pixels(
            overview_map_pixels,
            world_bounds,
            rasterization_palette,
            world_definitions,
            path_tiles,
            false,
        ),
        UiOverviewCanvas::ElevatedPaths => rasterize_path_tiles_into_overview_map_pixels(
            overview_map_pixels,
            world_bounds,
            rasterization_palette,
            world_definitions,
            path_tiles,
            true,
        ),
        UiOverviewCanvas::GroundTrack => {
            rasterize_transport_track_segments_into_overview_map_pixels(
                overview_map_pixels,
                world_bounds,
                rasterization_palette.ground_track,
                TransportationTrackKind::Ground,
                transport_track_segments,
                topology_node_transforms,
                world_definitions,
            )
        }
        UiOverviewCanvas::SkyTrack => rasterize_transport_track_segments_into_overview_map_pixels(
            overview_map_pixels,
            world_bounds,
            rasterization_palette.sky_track,
            TransportationTrackKind::Sky,
            transport_track_segments,
            topology_node_transforms,
            world_definitions,
        ),
    }
}
