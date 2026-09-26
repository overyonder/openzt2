use bevy::prelude::*;
use openzt2_game_data::ui_document::overview_map_presentation::UiMapColorsRecord;
use openzt2_game_data::{
    world_definitions::{
        transportation_and_tours::TransportationTrackKind,
        world_objects::WorldObjectInformationViewClass,
    },
    AssetId,
};

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::topology::topology_graph_types::FenceEdge;
use crate::plugins::topology::topology_graph_types::PathTile;
use crate::plugins::transport_tours::transport_topology_types::TrackSegment;
use crate::plugins::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

use super::overview_map_pixel_operations::{
    convert_world_position_to_overview_map_pixel, draw_overview_map_line_pixels,
    rasterize_world_sample_area_into_overview_map_pixels,
};

pub(super) fn rasterize_fence_edges_into_overview_map_pixels(
    overview_map_pixels: &mut [u8],
    world_bounds: &WorldTerrainHorizontalBounds,
    rasterization_palette: &UiMapColorsRecord,
    world_definitions: WorldDefinitionsView<'_>,
    fence_edges: &Query<&FenceEdge>,
    topology_node_transforms: &Query<&GlobalTransform>,
) {
    for fence_edge in fence_edges {
        let fence_view_class = world_definitions
            .find_fence(fence_edge.definition)
            .and_then(|definition| world_definitions.find_object(AssetId(definition.object.0)))
            .and_then(|object| object.view_class.as_ref());
        let fence_pixel_color = match fence_view_class {
            Some(WorldObjectInformationViewClass::Curb) => rasterization_palette.curb,
            Some(WorldObjectInformationViewClass::ZooWall) => rasterization_palette.zoo_wall,
            _ => rasterization_palette.fence,
        };
        let (Ok(start_node_transform), Ok(end_node_transform)) = (
            topology_node_transforms.get(fence_edge.a),
            topology_node_transforms.get(fence_edge.b),
        ) else {
            continue;
        };
        let start_pixel = convert_world_position_to_overview_map_pixel(
            world_bounds,
            start_node_transform.translation().xz(),
        );
        let end_pixel = convert_world_position_to_overview_map_pixel(
            world_bounds,
            end_node_transform.translation().xz(),
        );
        draw_overview_map_line_pixels(
            overview_map_pixels,
            start_pixel,
            end_pixel,
            fence_pixel_color,
        );
    }
}

pub(super) fn rasterize_path_tiles_into_overview_map_pixels(
    overview_map_pixels: &mut [u8],
    world_bounds: &WorldTerrainHorizontalBounds,
    rasterization_palette: &UiMapColorsRecord,
    world_definitions: WorldDefinitionsView<'_>,
    path_tiles: &Query<(&PathTile, &GlobalTransform)>,
    rasterize_elevated_paths: bool,
) {
    for (path_tile, path_transform) in path_tiles {
        let Some(path_definition) = world_definitions.find_path(path_tile.definition) else {
            continue;
        };
        if path_definition.elevated != rasterize_elevated_paths {
            continue;
        }
        let path_pixel_color = if path_definition.elevated {
            rasterization_palette.elevated_path
        } else {
            rasterization_palette.path
        };
        rasterize_world_sample_area_into_overview_map_pixels(
            overview_map_pixels,
            world_bounds,
            path_transform.translation().xz(),
            f32::from(path_definition.width_cm) * 0.01,
            path_pixel_color,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn rasterize_transport_track_segments_into_overview_map_pixels(
    overview_map_pixels: &mut [u8],
    world_bounds: &WorldTerrainHorizontalBounds,
    track_pixel_color: [u8; 4],
    requested_track_kind: TransportationTrackKind,
    transport_track_segments: &Query<&TrackSegment>,
    topology_node_transforms: &Query<&GlobalTransform>,
    world_definitions: WorldDefinitionsView<'_>,
) {
    for track_segment in transport_track_segments {
        let Some(track_definition) = world_definitions.find_track(track_segment.definition) else {
            continue;
        };
        if !matches!(
            (&track_definition.kind, &requested_track_kind),
            (
                TransportationTrackKind::Ground,
                TransportationTrackKind::Ground
            ) | (TransportationTrackKind::Sky, TransportationTrackKind::Sky)
        ) {
            continue;
        }
        let (Ok(start_node_transform), Ok(end_node_transform)) = (
            topology_node_transforms.get(track_segment.from),
            topology_node_transforms.get(track_segment.to),
        ) else {
            continue;
        };
        let start_world_position = start_node_transform.translation();
        let end_world_position = end_node_transform.translation();
        draw_overview_map_line_pixels(
            overview_map_pixels,
            convert_world_position_to_overview_map_pixel(world_bounds, start_world_position.xz()),
            convert_world_position_to_overview_map_pixel(world_bounds, end_world_position.xz()),
            track_pixel_color,
        );
    }
}
