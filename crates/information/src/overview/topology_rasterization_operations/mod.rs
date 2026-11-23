use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::{
    ui_document::overview_map_presentation::{UiMapColorsRecord, UiOverviewCanvas},
    world_definitions::{
        transportation_and_tours::TransportationTrackKind,
        world_objects::WorldObjectInformationViewClass,
    },
};
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_zoo_state::{
    topology::topology_graph_types::{FenceSegment, PathTile},
    transport_tours::transport_topology_types::TrackRoute,
};

use super::overview_map_pixel_operations::{
    convert_world_position_to_overview_map_pixel, draw_overview_map_line_pixels,
    rasterize_world_sample_area_into_overview_map_pixels,
};

pub(super) struct OverviewTopologyRaster {
    pub(super) source: UiOverviewCanvas,
    pub(super) palette: UiMapColorsRecord,
    pub(super) canvases: Vec<Entity>,
    pub(super) pixels: Vec<u8>,
}

pub(super) fn rasterize_fence_edges_into_overview_map_pixels(
    outputs: &mut [OverviewTopologyRaster],
    bounds: &WorldTerrainHorizontalBounds,
    definitions: WorldDefinitionsView<'_>,
    fences: &Query<FenceSegment>,
    nodes: &Query<&GlobalTransform>,
) {
    if !outputs
        .iter()
        .any(|output| output.source == UiOverviewCanvas::Fences)
    {
        return;
    }
    for fence in fences {
        let class = definitions
            .find_fence(fence.definition)
            .and_then(|definition| definitions.find_object(definition.object))
            .and_then(|object| object.view_class.as_ref());
        let (Ok(start), Ok(end)) = (nodes.get(fence.a()), nodes.get(fence.b())) else {
            continue;
        };
        let start = convert_world_position_to_overview_map_pixel(bounds, start.translation().xz());
        let end = convert_world_position_to_overview_map_pixel(bounds, end.translation().xz());
        for output in outputs
            .iter_mut()
            .filter(|output| output.source == UiOverviewCanvas::Fences)
        {
            let color = match class {
                Some(WorldObjectInformationViewClass::Curb) => output.palette.curb,
                Some(WorldObjectInformationViewClass::ZooWall) => output.palette.zoo_wall,
                _ => output.palette.fence,
            };
            draw_overview_map_line_pixels(&mut output.pixels, start, end, color);
        }
    }
}

pub(super) fn rasterize_path_tiles_into_overview_map_pixels(
    outputs: &mut [OverviewTopologyRaster],
    bounds: &WorldTerrainHorizontalBounds,
    definitions: WorldDefinitionsView<'_>,
    paths: &Query<(&PathTile, &GlobalTransform)>,
) {
    if !outputs.iter().any(|output| {
        matches!(
            output.source,
            UiOverviewCanvas::Paths | UiOverviewCanvas::ElevatedPaths
        )
    }) {
        return;
    }
    for (path, transform) in paths {
        let Some(definition) = definitions.find_path(path.definition) else {
            continue;
        };
        let source = if definition.elevated {
            UiOverviewCanvas::ElevatedPaths
        } else {
            UiOverviewCanvas::Paths
        };
        for output in outputs.iter_mut().filter(|output| output.source == source) {
            let color = if definition.elevated {
                output.palette.elevated_path
            } else {
                output.palette.path
            };
            rasterize_world_sample_area_into_overview_map_pixels(
                &mut output.pixels,
                bounds,
                transform.translation().xz(),
                f32::from(definition.width_cm) * 0.01,
                color,
            );
        }
    }
}

pub(super) fn rasterize_transport_track_segments_into_overview_map_pixels(
    outputs: &mut [OverviewTopologyRaster],
    bounds: &WorldTerrainHorizontalBounds,
    definitions: WorldDefinitionsView<'_>,
    tracks: &Query<TrackRoute>,
    nodes: &Query<&GlobalTransform>,
) {
    if !outputs.iter().any(|output| {
        matches!(
            output.source,
            UiOverviewCanvas::GroundTrack | UiOverviewCanvas::SkyTrack
        )
    }) {
        return;
    }
    for track in tracks {
        let Some(definition) = definitions.find_track(track.definition) else {
            continue;
        };
        let source = match definition.kind {
            TransportationTrackKind::Ground => UiOverviewCanvas::GroundTrack,
            TransportationTrackKind::Sky => UiOverviewCanvas::SkyTrack,
        };
        let (Ok(start), Ok(end)) = (nodes.get(track.from()), nodes.get(track.to())) else {
            continue;
        };
        let start = convert_world_position_to_overview_map_pixel(bounds, start.translation().xz());
        let end = convert_world_position_to_overview_map_pixel(bounds, end.translation().xz());
        for output in outputs.iter_mut().filter(|output| output.source == source) {
            let color = if source == UiOverviewCanvas::GroundTrack {
                output.palette.ground_track
            } else {
                output.palette.sky_track
            };
            draw_overview_map_line_pixels(&mut output.pixels, start, end, color);
        }
    }
}
