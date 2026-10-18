use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitted;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use crate::topology::fence_anchor_press_diagnostic_types::InitialFenceAnchorPress;
use crate::topology::fence_anchor_press_diagnostic_types::InitialFenceAnchorPressDiagnosticState;
use crate::topology::topology_construction_planning::calculate_fence_construction_plan;
use crate::topology::topology_construction_planning::calculate_path_construction_plan;
use crate::topology::topology_construction_planning::{
    GateHostEdges, calculate_gate_replacement_plan, fence_definition_is_gate,
};
use crate::topology::topology_construction_preview_types::FencePreview;
use crate::topology::topology_construction_preview_types::PathPreview;
use crate::topology::topology_construction_preview_types::PlaceFence;
use crate::topology::topology_construction_preview_types::PlacePath;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::AssetId;
    use openzt2_zoo_state::economy::money_types::Money;
    use openzt2_zoo_state::topology::topology_graph_types::EdgeKey;

    #[test]
    fn next_fence_run_starts_at_built_endpoint_instead_of_unsnapped_pointer() {
        let definition = AssetId([1; 16]);
        let accepted_endpoint = IVec3::new(10, 5, 0);
        let mut app = App::new();
        app.add_message::<ConstructionCommitted>()
            .insert_resource(ConstructionTool::Fence(definition))
            .init_resource::<TopologyGrid>()
            .add_systems(Update, restart_fence_preview_from_committed_endpoint);
        app.world_mut().spawn((
            PersistentId(2),
            TopologyNode {
                cell: accepted_endpoint,
            },
        ));
        let transaction = app
            .world_mut()
            .spawn(TopologyEdit {
                created: vec![TopologySnapshot::Fence {
                    id: PersistentId(3),
                    definition,
                    a: PersistentId(1),
                    b: PersistentId(2),
                    gate: None,
                }]
                .into_boxed_slice(),
                removed: Box::default(),
            })
            .id();
        let preview = app
            .world_mut()
            .spawn((
                ConstructionPreview {
                    definition,
                    transform: Transform::IDENTITY,
                    validity: PlacementValidity::Pending,
                },
                FencePreview {
                    definition,
                    from: IVec3::ZERO,
                    to: IVec3::new(11, 7, 0),
                },
                Transform::IDENTITY,
            ))
            .id();
        app.world_mut().write_message(ConstructionCommitted {
            transaction,
            cost: Money(100),
            screen_position: Vec2::ZERO,
        });
        app.update();
        let fence = app
            .world()
            .get::<FencePreview>(preview)
            .expect("preview retained");
        assert_eq!(fence.from, accepted_endpoint);
        assert_eq!(fence.to, accepted_endpoint);
        assert_eq!(
            app.world()
                .get::<Transform>(preview)
                .expect("preview transform")
                .translation,
            TopologyGrid::default().cell_translation(accepted_endpoint)
        );
    }

    #[test]
    fn gate_pointer_picks_the_nearest_fence_segment_within_reach() {
        let grid = TopologyGrid::default();
        let mut index = TopologyIndex::default();
        let near = EdgeKey::new(IVec3::ZERO, IVec3::new(4, 0, 0)).unwrap();
        let far = EdgeKey::new(IVec3::new(0, 3, 0), IVec3::new(4, 3, 0)).unwrap();
        for edge in [near, far] {
            index.edges.insert(edge, Entity::PLACEHOLDER);
        }
        let above = |cell: IVec3, offset: Vec3| grid.cell_translation(cell) + offset;
        let beside_near = above(
            IVec3::new(2, 0, 0),
            (grid.cell_translation(IVec3::new(2, 1, 0))
                - grid.cell_translation(IVec3::new(2, 0, 0)))
                * 0.4,
        );
        assert_eq!(
            hovered_fence_edge(&index, grid, beside_near),
            Some((near.lo, near.hi))
        );
        assert_eq!(
            hovered_fence_edge(&index, grid, above(IVec3::new(20, 20, 0), Vec3::ZERO)),
            None
        );
    }
}

pub(super) fn apply_fence_placement_requests_to_construction_previews(
    mut commands: Commands,
    mut requests: MessageReader<PlaceFence>,
    mut previews: Query<&mut FencePreview, With<ConstructionPreview>>,
) {
    for request in requests.read() {
        let preview = FencePreview {
            definition: request.definition,
            from: request.from,
            to: request.to,
        };
        if let Ok(mut current) = previews.get_mut(request.preview) {
            *current = preview;
        } else if let Ok(mut entity) = commands.get_entity(request.preview) {
            entity.insert(preview);
        }
    }
}

pub(super) fn apply_path_placement_requests_to_construction_previews(
    mut commands: Commands,
    mut requests: MessageReader<PlacePath>,
    mut previews: Query<&mut PathPreview, With<ConstructionPreview>>,
) {
    for request in requests.read() {
        let preview = PathPreview {
            definition: request.definition,
            from: request.from,
            to: request.to,
        };
        if let Ok(mut current) = previews.get_mut(request.preview) {
            *current = preview;
        } else if let Ok(mut entity) = commands.get_entity(request.preview) {
            entity.insert(preview);
        }
    }
}

/// Owns the authored fence and ground-path click-anchor/click-commit transition.
/// The topology messages remain the only preview mutation input;
/// the construction transaction performs final validation.
pub fn drive_selected_topology_placement_pointer_sequence(
    mut commands: Commands,
    primary_pointer: Res<openzt2_interface_state::input::input_types::PrimaryPointerInputState>,
    ui_capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    tool: Res<ConstructionTool>,
    grid: Res<TopologyGrid>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    index: Res<TopologyIndex>,
    cursors: Query<&ConstructionCursor>,
    roots: Query<Entity, With<WorldRoot>>,
    previews: Query<
        (Entity, Option<&FencePreview>, Option<&PathPreview>),
        With<ConstructionPreview>,
    >,
    pending_transactions: Query<&EditCommitProgress>,
    mut fences: MessageWriter<PlaceFence>,
    mut paths: MessageWriter<PlacePath>,
    mut commits: MessageWriter<CommitConstruction>,
    mut anchor_diagnostics: Option<ResMut<InitialFenceAnchorPressDiagnosticState>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let definition = match *tool {
        ConstructionTool::Fence(definition) | ConstructionTool::Path(definition) => definition,
        _ => return,
    };
    if pending_transactions
        .iter()
        .any(|progress| progress.phase != EditCommitPhase::Complete)
    {
        return;
    }
    let (Ok(cursor), Ok(root)) = (cursors.single(), roots.single()) else {
        return;
    };
    if ui_capture.over_ui || !cursor.over_terrain {
        return;
    }
    let mut cell = grid.position_cell(cursor.world);
    let ground_aligned = match *tool {
        ConstructionTool::Fence(_) => true,
        ConstructionTool::Path(definition) => definitions
            .find_path(definition)
            .is_some_and(|path| !path.elevated),
        _ => false,
    };
    if ground_aligned {
        cell.z = 0;
    }
    let gate = matches!(*tool, ConstructionTool::Fence(definition)
        if fence_definition_is_gate(definitions, definition));
    let Ok((preview, fence, path)) = previews.single() else {
        if gate || primary_pointer.just_pressed && !ui_capture.over_ui {
            if let (ConstructionTool::Fence(definition), Some(diagnostics), false) =
                (*tool, anchor_diagnostics.as_mut(), gate)
            {
                diagnostics.record_press(InitialFenceAnchorPress {
                    root,
                    definition,
                    grid: *grid,
                    pointer: *primary_pointer,
                    cursor: *cursor,
                    ui_capture: *ui_capture,
                    cell,
                });
            }
            let transform = Transform::from_translation(grid.cell_translation(cell));
            let mut entity = commands.spawn((
                ConstructionPreview {
                    definition,
                    transform,
                    validity: PlacementValidity::Pending,
                },
                transform,
                Visibility::Inherited,
                WorldMember { root },
            ));
            match *tool {
                ConstructionTool::Fence(_) => {
                    entity.insert(FencePreview {
                        definition,
                        from: cell,
                        to: cell,
                    });
                }
                ConstructionTool::Path(_) => {
                    entity.insert(PathPreview {
                        definition,
                        from: cell,
                        to: cell,
                    });
                }
                _ => {}
            }
        }
        return;
    };
    if let (Some(fence), true) = (fence, gate) {
        let (from, to) = hovered_fence_edge(&index, *grid, cursor.world).unwrap_or((cell, cell));
        if !ui_capture.over_ui && (fence.from, fence.to) != (from, to) {
            fences.write(PlaceFence {
                preview,
                definition,
                from,
                to,
            });
        }
        if primary_pointer.just_pressed && !ui_capture.over_ui {
            commits.write(CommitConstruction { preview });
        }
    } else if let Some(fence) = fence {
        if !ui_capture.over_ui && fence.to != cell {
            fences.write(PlaceFence {
                preview,
                definition,
                from: fence.from,
                to: cell,
            });
        }
        if primary_pointer.just_pressed && !ui_capture.over_ui {
            commits.write(CommitConstruction { preview });
        }
    } else if let Some(path) = path {
        if !ui_capture.over_ui && path.to != cell && (ground_aligned || primary_pointer.pressed) {
            paths.write(PlacePath {
                preview,
                definition,
                from: path.from,
                to: cell,
            });
        }
        if !ui_capture.over_ui
            && if ground_aligned {
                primary_pointer.just_pressed
            } else {
                primary_pointer.just_released
            }
        {
            commits.write(CommitConstruction { preview });
        }
    }
}

/// The fence segment nearest the pointer, within reach of a gate click.
fn hovered_fence_edge(
    index: &TopologyIndex,
    grid: TopologyGrid,
    world: Vec3,
) -> Option<(IVec3, IVec3)> {
    const REACH_M: f32 = 1.5;
    let point = world.xz();
    index
        .edges
        .keys()
        .map(|edge| {
            let (lo, hi) = (
                grid.cell_translation(edge.lo).xz(),
                grid.cell_translation(edge.hi).xz(),
            );
            let along = (point - lo).dot(hi - lo) / (hi - lo).length_squared().max(f32::EPSILON);
            (edge, point.distance(lo + (hi - lo) * along.clamp(0.0, 1.0)))
        })
        .filter(|(_, distance)| *distance <= REACH_M)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(edge, _)| (edge.lo, edge.hi))
}

/// Keeps the committed endpoint as the next ground-path anchor. Pointer updates
/// pause during transaction application, so `to` remains the submitted endpoint.
pub(super) fn restart_path_preview_from_committed_endpoint(
    mut committed: MessageReader<ConstructionCommitted>,
    tool: Res<ConstructionTool>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    edits: Query<&TopologyEdit>,
    mut previews: Query<(&mut ConstructionPreview, &mut PathPreview)>,
) {
    let ConstructionTool::Path(definition) = *tool else {
        committed.clear();
        return;
    };
    if !active_definitions
        .get(&definitions)
        .is_some_and(|definitions| {
            definitions
                .find_path(definition)
                .is_some_and(|path| !path.elevated)
        })
    {
        committed.clear();
        return;
    }
    let Ok((mut construction, mut path)) = previews.single_mut() else {
        committed.clear();
        return;
    };
    for commit in committed.read() {
        let Ok(edit) = edits.get(commit.transaction) else {
            continue;
        };
        if edit.created.iter().any(|snapshot| {
            matches!(snapshot,
                TopologySnapshot::Path { definition: placed, .. } if *placed == definition
            )
        }) {
            path.from = path.to;
            construction.validity = PlacementValidity::Pending;
        }
    }
}

/// Continues authored freeform fence placement from the endpoint accepted by
/// the construction transaction. The existing preview entity remains the one
/// interaction-state owner; committed topology is owned independently by the
/// fence entities created from the transaction snapshots.
pub(super) fn restart_fence_preview_from_committed_endpoint(
    mut committed: MessageReader<ConstructionCommitted>,
    tool: Res<ConstructionTool>,
    grid: Res<TopologyGrid>,
    edits: Query<&TopologyEdit>,
    nodes: Query<(&PersistentId, &TopologyNode)>,
    mut previews: Query<(&mut ConstructionPreview, &mut FencePreview, &mut Transform)>,
) {
    let ConstructionTool::Fence(definition) = *tool else {
        committed.clear();
        return;
    };
    let Ok((mut construction, mut fence, mut transform)) = previews.single_mut() else {
        committed.clear();
        return;
    };
    for commit in committed.read() {
        let Ok(edit) = edits.get(commit.transaction) else {
            continue;
        };
        let Some(endpoint_id) = edit
            .created
            .iter()
            .rev()
            .find_map(|snapshot| match snapshot {
                TopologySnapshot::Fence {
                    definition: created_definition,
                    b,
                    ..
                } if *created_definition == definition => Some(*b),
                _ => None,
            })
        else {
            continue;
        };
        let Some(endpoint) = nodes
            .iter()
            .find_map(|(id, node)| (*id == endpoint_id).then_some(node.cell))
        else {
            continue;
        };
        // The pointer endpoint can lie beyond the final full-length segment.
        // Continue the chain at the node accepted by the transaction.
        fence.from = endpoint;
        fence.to = endpoint;
        construction.transform = Transform::from_translation(grid.cell_translation(endpoint));
        construction.validity = PlacementValidity::Pending;
        *transform = construction.transform;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate_topology_construction_preview_validity(
    grid: Res<TopologyGrid>,
    index: Res<TopologyIndex>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    terrain: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    gate_hosts: GateHostEdges,
    mut previews: Query<
        (
            &mut ConstructionPreview,
            Option<&FencePreview>,
            Option<&PathPreview>,
        ),
        Or<(Changed<FencePreview>, Changed<PathPreview>)>,
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (mut construction, fence, path) in &mut previews {
        let result = match (fence, path) {
            (Some(fence), None) if fence_definition_is_gate(definitions, fence.definition) => {
                calculate_gate_replacement_plan(fence, &index, definitions, &gate_hosts)
                    .map(|(_, cost)| cost)
            }
            (Some(fence), None) => calculate_fence_construction_plan(
                fence,
                &index,
                *grid,
                definitions,
                &terrain,
                &terrain_assets,
                &terrain_chunks,
            )
            .map(|(_, cost, _)| cost),
            (None, Some(path)) => calculate_path_construction_plan(
                path,
                &index,
                *grid,
                definitions,
                &terrain,
                &terrain_assets,
                &terrain_chunks,
            )
            .map(|(_, cost)| cost),
            _ => Err(PlacementFailure::InvalidTopology),
        };
        construction.validity = match result {
            Ok(cost) => PlacementValidity::Valid { cost },
            Err(reason) => PlacementValidity::Invalid(reason),
        };
    }
}
