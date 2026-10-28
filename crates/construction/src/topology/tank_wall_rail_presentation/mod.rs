//! Top rails of tank wall segments, drawn level at the wall top.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::fences_and_gates::FenceSegmentPrefabs;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankWallTopHeight;
use openzt2_zoo_state::topology::fence_segment_presentation_types::{FenceSegmentPrefab, FenceTerrainEndpointPresentationCells};
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyGrid, TopologyNode};
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_topology_edge_world_transform;

/// The rail drawn for one tank wall segment.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TankWallRailOf(pub Entity);

/// The original selects a wall's top rail by prefixing the selected segment
/// binder's name with `t`, so the rail takes the same shape slot as the
/// segment model.
fn select_tank_wall_rail(
    segments: &FenceSegmentPrefabs,
    rails: &FenceSegmentPrefabs,
    segment: AssetId,
) -> Option<AssetId> {
    [
        (segments.cardinal_straight, rails.cardinal_straight),
        (segments.diagonal_straight, rails.diagonal_straight),
        (segments.cardinal_curve_90, rails.cardinal_curve_90),
        (segments.diagonal_curve_90, rails.diagonal_curve_90),
        (segments.cardinal_curve_135, rails.cardinal_curve_135),
        (segments.diagonal_curve_135, rails.diagonal_curve_135),
    ]
    .into_iter()
    .find_map(|(segment_slot, rail)| (segment_slot == segment).then_some(rail))
    .filter(|rail| *rail != AssetId::default())
}

/// Rebuilds the rail of each tank wall whose top or segment model changed.
#[allow(clippy::type_complexity)]
pub(super) fn present_changed_tank_wall_rails(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    walls: Query<(
        Entity,
        &FenceEdge,
        Ref<FenceSegmentPrefab>,
        Ref<TankWallTopHeight>,
        Option<&FenceTerrainEndpointPresentationCells>,
        Has<Gate>,
        &WorldMember,
    )>,
    nodes: Query<&TopologyNode>,
    rails: Query<(Entity, &TankWallRailOf)>,
) {
    let definitions_changed = definitions.is_changed() || active_definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (wall, edge, segment, top, endpoint_cells, gate, member) in &walls {
        if !definitions_changed
            && !segment.is_changed()
            && !top.is_changed()
            && rails.iter().any(|(_, of)| of.0 == wall)
        {
            continue;
        }
        for (rail, of) in &rails {
            if of.0 == wall {
                commands.entity(rail).despawn();
            }
        }
        let fence = if gate {
            definitions.find_gate_fence(edge.definition)
        } else {
            definitions.find_fence(edge.definition)
        };
        let Some(fence) = fence else {
            continue;
        };
        let Some(rail) = fence
            .top_segments
            .as_ref()
            .and_then(|rails| select_tank_wall_rail(&fence.segments, rails, segment.asset))
        else {
            continue;
        };
        let (Ok(a), Ok(b), Some(handle)) = (
            nodes.get(edge.a),
            nodes.get(edge.b),
            definitions.scene(rail),
        ) else {
            continue;
        };
        let (first, second) =
            endpoint_cells.map_or((a.cell, b.cell), |cells| (cells.first, cells.second));
        let mut transform = calculate_topology_edge_world_transform(first, second, *grid);
        transform.translation.y = top.0;
        if segment.mirror_source_y {
            transform.scale.z = -1.0;
        }
        commands.spawn((
            TankWallRailOf(wall),
            transform,
            Visibility::Inherited,
            *member,
            PrefabPresentation::new(handle),
        ));
    }
}

/// Removes rails whose wall is gone or no longer has a top.
pub(super) fn remove_orphaned_tank_wall_rails(
    mut commands: Commands,
    rails: Query<(Entity, &TankWallRailOf)>,
    walls: Query<(), (With<FenceEdge>, With<TankWallTopHeight>)>,
) {
    for (rail, of) in &rails {
        if walls.get(of.0).is_err() {
            commands.entity(rail).despawn();
        }
    }
}
