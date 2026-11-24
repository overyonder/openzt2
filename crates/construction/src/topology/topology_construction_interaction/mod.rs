use super::elevated_curb_layout::{is_elevated_curb, ElevatedCurbSurfaces};
use super::elevated_path_layout::{ElevatedPathMode, ElevatedPathProfile};
use super::topology_construction_planning::{
    calculate_ground_path_construction_cells, validate_path_construction_cells,
};
use super::topology_construction_preview_types::{FenceDrawPlan, PathDrawPlan};
use crate::construction::construction_lifecycle::ConstructionActive;
use bevy::ecs::system::SystemParam;
use bevy::picking::pointer::{PointerButton, PointerInput};
use bevy::prelude::*;
use openzt2_interface_state::input::input_types::MousePointer;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_zoo_state::progression::{
    adoption_and_content_availability_types::ScenarioContentAvailability,
    catalogue_entry_availability::catalogue_entry_is_available,
    unlock_types::UnlockedCatalogueDefinitionSet,
};
use openzt2_zoo_state::topology::topology_graph_types::FenceSegment;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::FencePlacementShape;
use openzt2_interface_state::information::catalogue_types::PurchaseChoice;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitted;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use crate::topology::fence_anchor_press_diagnostic_types::InitialFenceAnchorPress;
use crate::topology::fence_anchor_press_diagnostic_types::InitialFenceAnchorPressDiagnosticState;
use crate::topology::topology_construction_planning::calculate_fence_construction_plan;
use crate::topology::topology_construction_planning::calculate_path_construction_plan;
use crate::topology::topology_construction_planning::{
    calculate_gate_replacement_plan, fence_definition_is_gate, GateHostEdges,
};
use crate::topology::topology_construction_preview_types::FencePreview;
use crate::topology::topology_construction_preview_types::PathPreview;
use crate::topology::topology_construction_preview_types::PlaceFence;
use crate::topology::topology_construction_preview_types::PlacePath;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot;
use openzt2_zoo_state::topology::topology_edit_types::{AppliedFenceTopology, TopologyEdit};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_graph_types::{EdgeKey, FenceEdge};

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::AssetId;
    use openzt2_zoo_state::economy::money_types::Money;
    #[test]
    fn next_fence_run_starts_at_built_endpoint_instead_of_unsnapped_pointer() {
        let definition = AssetId([1; 16]);
        let accepted_endpoint = IVec3::new(10, 5, 0);
        let mut app = App::new();
        app.add_message::<ConstructionCommitted>()
            .insert_resource(ConstructionTool::Fence(definition))
            .add_systems(Update, restart_fence_preview_from_committed_endpoint);
        app.world_mut().spawn(TopologyGrid::default());
        let endpoint = app
            .world_mut()
            .spawn(TopologyNode {
                cell: accepted_endpoint,
            })
            .id();
        let start = app
            .world_mut()
            .spawn(TopologyNode { cell: IVec3::ZERO })
            .id();
        let wall = app
            .world_mut()
            .spawn(FenceEdge::between(definition, start, endpoint))
            .id();
        let transaction = app
            .world_mut()
            .spawn(AppliedFenceTopology(vec![wall].into_boxed_slice()))
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
                    shape: FencePlacementShape::Freeform,
                    heading_eighth_turns: 0,
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
    fn gate_pointer_picks_the_nearest_fence_segment_or_prompted_enclosure_wall() {
        let grid = TopologyGrid::default();
        let mut world = World::new();
        let root = world.spawn(TopologyIndex::default()).id();
        let near = EdgeKey::new(IVec3::ZERO, IVec3::new(4, 0, 0)).unwrap();
        let far = EdgeKey::new(IVec3::new(0, 3, 0), IVec3::new(4, 3, 0)).unwrap();
        for edge in [near, far] {
            let a = world
                .spawn((TopologyNode { cell: edge.lo }, WorldMember { root }))
                .id();
            let b = world
                .spawn((TopologyNode { cell: edge.hi }, WorldMember { root }))
                .id();
            world.spawn((
                FenceEdge::between(AssetId::default(), a, b),
                WorldMember { root },
            ));
        }
        let index = world.get::<TopologyIndex>(root).unwrap();
        let above = |cell: IVec3, offset: Vec3| grid.cell_translation(cell) + offset;
        let beside_near = above(
            IVec3::new(2, 0, 0),
            (grid.cell_translation(IVec3::new(2, 1, 0))
                - grid.cell_translation(IVec3::new(2, 0, 0)))
                * 0.4,
        );
        assert_eq!(
            hovered_fence_edge(index, grid, beside_near),
            Some((near.lo, near.hi))
        );
        assert_eq!(
            hovered_fence_edge(index, grid, above(IVec3::new(20, 20, 0), Vec3::ZERO)),
            None
        );
        assert_eq!(
            nearest_fence_edge(
                std::iter::once(far),
                grid,
                above(IVec3::new(20, 20, 0), Vec3::ZERO),
                f32::INFINITY,
            ),
            Some((far.lo, far.hi))
        );
    }
}

pub(super) fn apply_fence_placement_requests_to_construction_previews(
    mut commands: Commands,
    mut requests: MessageReader<PlaceFence>,
    mut previews: Query<&mut FencePreview, With<ConstructionPreview>>,
) {
    for request in requests.read() {
        let mut preview = FencePreview {
            definition: request.definition,
            from: request.from,
            to: request.to,
            shape: FencePlacementShape::Freeform,
            heading_eighth_turns: 0,
        };
        if let Ok(mut current) = previews.get_mut(request.preview) {
            preview.shape = current.shape;
            preview.heading_eighth_turns = current.heading_eighth_turns;
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
            profile: request.profile,
        };
        if let Ok(mut current) = previews.get_mut(request.preview) {
            current.set_if_neq(preview);
        } else if let Ok(mut entity) = commands.get_entity(request.preview) {
            entity.insert(preview);
        }
    }
}

/// Owns the authored fence and ground-path click-anchor/click-commit transition.
/// The topology messages remain the only preview mutation input;
/// the construction transaction performs final validation.
pub(super) fn drive_selected_topology_placement_pointer_sequence(
    mut commands: Commands,
    (pointer, mut pointer_inputs): (MousePointer, MessageReader<PointerInput>),
    ui_capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    (tool, policy): (Res<ConstructionTool>, Res<ConstructionPlacementPolicy>),
    grid: Single<&TopologyGrid>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    index: Single<&TopologyIndex>,
    cursors: Query<&ConstructionCursor>,
    roots: Query<
        (
            Entity,
            Option<&SelectedWorldIdentity>,
            Option<&ScenarioContentAvailability>,
            Option<&UnlockedCatalogueDefinitionSet>,
        ),
        With<WorldRoot>,
    >,
    previews: Query<
        (Entity, Option<&FencePreview>, Option<&PathPreview>),
        With<ConstructionPreview>,
    >,
    pending_transactions: Query<(), With<ConstructionActive>>,
    mut fences: MessageWriter<PlaceFence>,
    mut paths: MessageWriter<PlacePath>,
    mut commits: MessageWriter<CommitConstruction>,
    (mut anchor_diagnostics, gate_prompt, curb_surfaces, elevated_mode): (
        Option<ResMut<InitialFenceAnchorPressDiagnosticState>>,
        GatePromptQueries,
        ElevatedCurbSurfaces,
        ElevatedPathMode,
    ),
) {
    let grid = grid.into_inner();
    let index = index.into_inner();
    let mut primary_just_released = false;
    let mut primary_press = None;
    for input in pointer_inputs
        .read()
        .filter(|input| input.pointer_id.is_mouse())
    {
        primary_just_released |= input.button_just_released(PointerButton::Primary);
        if input.button_just_pressed(PointerButton::Primary) {
            primary_press = Some(input);
        }
    }
    let primary_just_pressed = primary_press.is_some();
    let primary_pressed = pointer.primary_is_pressed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    if !pending_transactions.is_empty() {
        return;
    }
    let (Ok(cursor), Ok((root, session, scenario, unlocked))) = (cursors.single(), roots.single())
    else {
        return;
    };
    let definition = match *tool {
        ConstructionTool::ElevatedCurbPlacement | ConstructionTool::ElevatedPathPlacement => {
            let Some(unlocked) = unlocked else {
                return;
            };
            let selected =
                definitions
                    .catalogue_in_authored_purchase_order()
                    .find_map(|(index, entry)| {
                        let id = entry.definition;
                        let matching = if *tool == ConstructionTool::ElevatedCurbPlacement {
                            is_elevated_curb(definitions, id)
                        } else {
                            definitions.find_path(id).is_some_and(|path| path.elevated)
                        };
                        (matching
                            && catalogue_entry_is_available(
                                definitions,
                                index,
                                entry,
                                session.map(|session| session.mode),
                                scenario,
                                unlocked,
                            ))
                        .then_some(id)
                    });
            let Some(definition) = selected else {
                return;
            };
            definition
        }
        ConstructionTool::Fence(definition) | ConstructionTool::Path(definition) => definition,
        _ => return,
    };
    if ui_capture.over_ui || !cursor.over_terrain {
        return;
    }
    let mut cell = grid.position_cell(cursor.world);
    let curb = is_elevated_curb(definitions, definition);
    if curb {
        let Some(endpoint) = curb_surfaces.nearest(cursor.world, *grid, index, definitions) else {
            return;
        };
        cell = endpoint;
    }
    let ground_aligned = match *tool {
        ConstructionTool::Fence(_) => !curb,
        ConstructionTool::Path(definition) => definitions
            .find_path(definition)
            .is_some_and(|path| !path.elevated),
        _ => false,
    };
    if ground_aligned {
        cell.z = 0;
    }
    let path_definition = definitions
        .find_path(definition)
        .filter(|definition| definition.elevated);
    let elevation = |base_m| {
        Some(ElevatedPathProfile::new(
            base_m,
            path_definition?,
            definitions.topology_cell_size_cm(),
            elevated_mode.clearance_cm()?,
            &policy,
        ))
    };
    if path_definition.is_some() && elevated_mode.clearance_cm().is_none() {
        return;
    }

    let gate = matches!(*tool, ConstructionTool::Fence(definition)
        if fence_definition_is_gate(definitions, definition));
    let Ok((preview, fence, path)) = previews.single() else {
        if gate || primary_just_pressed && !ui_capture.over_ui {
            if let (ConstructionTool::Fence(definition), Some(diagnostics), Some(pointer), false) =
                (*tool, anchor_diagnostics.as_mut(), primary_press, gate)
            {
                diagnostics.record_press(InitialFenceAnchorPress {
                    root,
                    definition,
                    grid: *grid,
                    pointer: pointer.clone(),
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
                ConstructionTool::Fence(_) | ConstructionTool::ElevatedCurbPlacement => {
                    entity.insert(FencePreview {
                        definition,
                        from: cell,
                        to: cell,
                        shape: if gate || curb {
                            FencePlacementShape::Freeform
                        } else {
                            policy.fence_shape
                        },
                        heading_eighth_turns: policy.fence_heading_eighth_turns,
                    });
                }
                ConstructionTool::Path(_) | ConstructionTool::ElevatedPathPlacement => {
                    entity.insert(PathPreview {
                        definition,
                        from: cell,
                        to: cell,
                        profile: elevation(cursor.world.y),
                    });
                }
                _ => {}
            }
        }
        return;
    };
    if let (Some(fence), true) = (fence, gate) {
        let (from, to) = gate_prompt
            .nearest_wall(*grid, cursor.world)
            .or_else(|| hovered_fence_edge(&index, *grid, cursor.world))
            .unwrap_or((cell, cell));
        if !ui_capture.over_ui && (fence.from, fence.to) != (from, to) {
            fences.write(PlaceFence {
                preview,
                definition,
                from,
                to,
            });
        }
        if primary_just_pressed && !ui_capture.over_ui {
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
        if primary_just_pressed && !ui_capture.over_ui {
            commits.write(CommitConstruction { preview });
        }
    } else if let Some(path) = path {
        let profile = elevation(
            path.profile
                .map_or(path.from.z as f32, |profile| profile.reference_m()),
        );
        if !ui_capture.over_ui
            && (path.to != cell || path.profile != profile)
            && (ground_aligned || primary_pressed || primary_just_released || policy.is_changed())
        {
            paths.write(PlacePath {
                preview,
                definition,
                from: path.from,
                to: cell,
                profile,
            });
        }
        if !ui_capture.over_ui
            && if ground_aligned {
                primary_just_pressed
            } else {
                primary_just_released
            }
        {
            commits.write(CommitConstruction { preview });
        }
    }
}

#[derive(Component)]
pub(super) struct RectangularEnclosureGatePrompt;

#[derive(Component)]
#[relationship(relationship_target = GatePromptWallCandidates)]
pub(super) struct GatePromptCandidateWall(Entity);

#[derive(Component)]
#[relationship_target(relationship = GatePromptCandidateWall, linked_spawn)]
pub(super) struct GatePromptWallCandidates(Vec<Entity>);

#[derive(SystemParam)]
pub(super) struct GatePromptQueries<'w, 's> {
    prompts: Query<'w, 's, &'static Children, With<RectangularEnclosureGatePrompt>>,
    candidates: Query<'w, 's, &'static GatePromptCandidateWall>,
    walls: Query<'w, 's, FenceSegment>,
    nodes: Query<'w, 's, &'static TopologyNode>,
}

impl GatePromptQueries<'_, '_> {
    fn nearest_wall(&self, grid: TopologyGrid, world: Vec3) -> Option<(IVec3, IVec3)> {
        let children = self.prompts.single().ok()?;
        let keys = children.iter().filter_map(|candidate| {
            let wall = self
                .walls
                .get(self.candidates.get(candidate).ok()?.0)
                .ok()?;
            let first = self.nodes.get(wall.a()).ok()?;
            let second = self.nodes.get(wall.b()).ok()?;
            EdgeKey::new(first.cell, second.cell)
        });
        nearest_fence_edge(keys, grid, world, f32::INFINITY)
    }
}

pub(super) fn drop_prompt_candidates_when_fence_component_is_removed(
    removed: On<Remove<FenceEdge>>,
    walls: Query<&GatePromptWallCandidates>,
    mut commands: Commands,
) {
    if let Ok(candidates) = walls.get(removed.entity) {
        for candidate in candidates.iter() {
            commands.entity(candidate).try_despawn();
        }
    }
}

pub(super) fn track_rectangular_enclosure_gate_prompt(
    mut commands: Commands,
    mut committed: MessageReader<ConstructionCommitted>,
    tool: Res<ConstructionTool>,
    mut policy: ResMut<ConstructionPlacementPolicy>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    applied: Query<&AppliedFenceTopology>,
    walls: Query<(FenceSegment, &WorldMember, Has<Gate>)>,
    prompts: Query<Entity, With<RectangularEnclosureGatePrompt>>,
    mut purchases: MessageWriter<PurchaseChoice>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        committed.clear();
        return;
    };
    let gate_tool = matches!(*tool, ConstructionTool::Fence(definition)
        if fence_definition_is_gate(definitions, definition));
    let mut prompt = prompts.single().ok();
    if tool.is_changed() && !gate_tool {
        if let Some(entity) = prompt.take() {
            commands.entity(entity).despawn();
        }
    }
    for commit in committed.read() {
        let Ok(applied) = applied.get(commit.transaction) else {
            continue;
        };
        if gate_tool
            && applied
                .0
                .iter()
                .any(|entity| walls.get(*entity).is_ok_and(|(_, _, gate)| gate))
        {
            if let Some(entity) = prompt.take() {
                commands.entity(entity).despawn();
                let wizard = definitions
                    .fence_placement_mode_policy()
                    .and_then(|mode| mode.gate_wizard_placement_object)
                    .filter(|object| definitions.find_placeable(*object).is_some());
                if let Some(definition) = wizard {
                    purchases.write(PurchaseChoice { definition });
                    policy.repeat_placement = false;
                }
            }
            continue;
        }
        if gate_tool || policy.fence_shape == FencePlacementShape::Freeform {
            continue;
        }
        let candidates = applied
            .0
            .iter()
            .copied()
            .filter_map(|entity| {
                let (wall, membership, is_gate) = walls.get(entity).ok()?;
                (!is_gate
                    && definitions
                        .find_fence(wall.definition)
                        .is_some_and(|fence| fence.gate != AssetId::default()))
                .then_some((entity, *membership))
            })
            .collect::<Vec<_>>();
        if let Some((_, membership)) = candidates.first() {
            if let Some(previous) = prompt {
                commands.entity(previous).despawn();
            }
            let entity = commands
                .spawn((RectangularEnclosureGatePrompt, *membership))
                .id();
            prompt = Some(entity);
            for (wall, _) in candidates {
                commands.spawn((ChildOf(entity), GatePromptCandidateWall(wall)));
            }
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
    if !world.is_finite() || !grid.spacing_m.is_finite() || grid.spacing_m <= 0.0 {
        return None;
    }
    let point = world.xz() - grid.origin;
    let min = ((point - Vec2::splat(REACH_M)) / grid.spacing_m)
        .floor()
        .as_ivec2();
    let max = ((point + Vec2::splat(REACH_M)) / grid.spacing_m)
        .ceil()
        .as_ivec2();
    let edges = index
        .fences_within(min, max)
        .filter_map(|entity| index.fence_key(entity));
    nearest_fence_edge(edges, grid, world, REACH_M)
}

fn nearest_fence_edge(
    edges: impl Iterator<Item = EdgeKey>,
    grid: TopologyGrid,
    world: Vec3,
    reach_m: f32,
) -> Option<(IVec3, IVec3)> {
    let point = world.xz();
    edges
        .map(|edge| {
            let (lo, hi) = (
                grid.cell_translation(edge.lo).xz(),
                grid.cell_translation(edge.hi).xz(),
            );
            let along = (point - lo).dot(hi - lo) / (hi - lo).length_squared().max(f32::EPSILON);
            (edge, point.distance(lo + (hi - lo) * along.clamp(0.0, 1.0)))
        })
        .filter(|(_, distance)| *distance <= reach_m)
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
    mut commands: Commands,
    mut committed: MessageReader<ConstructionCommitted>,
    tool: Res<ConstructionTool>,
    grid: Single<&TopologyGrid>,
    applied: Query<&AppliedFenceTopology>,
    walls: Query<FenceSegment>,
    nodes: Query<&TopologyNode>,
    mut previews: Query<(
        Entity,
        &mut ConstructionPreview,
        &mut FencePreview,
        &mut Transform,
    )>,
) {
    let grid = grid.into_inner();
    let Ok((preview, mut construction, mut fence, mut transform)) = previews.single_mut() else {
        committed.clear();
        return;
    };
    let definition = match *tool {
        ConstructionTool::Fence(definition) => definition,
        ConstructionTool::ElevatedCurbPlacement => fence.definition,
        _ => {
            committed.clear();
            return;
        }
    };
    // A closed shape is complete; the next one starts from a new anchor.
    if fence.shape != FencePlacementShape::Freeform {
        if committed.read().count() != 0 {
            commands.entity(preview).despawn();
        }
        return;
    }
    for commit in committed.read() {
        let Some(endpoint) = applied.get(commit.transaction).ok().and_then(|applied| {
            applied
                .0
                .iter()
                .rev()
                .filter_map(|wall| walls.get(*wall).ok())
                .find(|wall| wall.definition == definition)
                .and_then(|wall| nodes.get(wall.b()).ok())
                .map(|node| node.cell)
        }) else {
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
    mut commands: Commands,
    grid: Single<Ref<TopologyGrid>>,
    index: Single<Ref<TopologyIndex>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    terrain: Single<Ref<TerrainIndex>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    mut changes: MessageReader<openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    (gate_hosts, curb_surfaces): (GateHostEdges, ElevatedCurbSurfaces),
    mut previews: Query<
        (
            Entity,
            &mut ConstructionPreview,
            Option<Ref<FencePreview>>,
            Option<Ref<PathPreview>>,
            Option<&mut FenceDrawPlan>,
            Option<&mut PathDrawPlan>,
        ),
        Or<(With<FencePreview>, With<PathPreview>)>,
    >,
) {
    let grid = grid.into_inner();
    let index = index.into_inner();
    let terrain = terrain.into_inner();
    let terrain_changed = changes.read().count() != 0;
    let dirty = terrain_changed
        || grid.is_changed()
        || index.is_changed()
        || terrain.is_changed()
        || terrain_assets.is_changed()
        || active_definitions.is_changed()
        || definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, mut preview, fence, path, fence_plan, path_plan) in &mut previews {
        let validity = |result: Result<
            openzt2_zoo_state::economy::money_types::Money,
            PlacementFailure,
        >| match result {
            Ok(cost) => PlacementValidity::Valid { cost },
            Err(reason) => PlacementValidity::Invalid(reason),
        };
        let next = if let Some(fence) = fence {
            if !dirty && !fence.is_changed() && fence_plan.is_some() {
                continue;
            }
            let gate = fence_definition_is_gate(definitions, fence.definition);
            let result = if gate {
                calculate_gate_replacement_plan(&fence, &index, definitions, &gate_hosts)
                    .map(|(host, cost)| (vec![fence.from, fence.to], cost, Some(host)))
            } else {
                calculate_fence_construction_plan(
                    &fence,
                    &index,
                    *grid,
                    definitions,
                    &terrain,
                    &terrain_assets,
                    &terrain_chunks,
                    &gate_hosts,
                    Some(&curb_surfaces),
                )
                .map(|(cells, cost, ..)| (cells, cost, None))
            };
            let (plan, validity) = match result {
                Ok((cells, cost, replaced)) => {
                    (FenceDrawPlan { cells, replaced }, validity(Ok(cost)))
                }
                Err(reason) => (
                    FenceDrawPlan {
                        cells: Vec::new(),
                        replaced: None,
                    },
                    validity(Err(reason)),
                ),
            };
            if let Some(mut existing) = fence_plan {
                existing.set_if_neq(plan);
            } else {
                commands.entity(entity).insert(plan);
            }
            validity
        } else if let Some(path) = path {
            if !dirty && !path.is_changed() && path_plan.is_some() {
                continue;
            }
            let (cells, next) = if let Some(definition) = definitions
                .find_path(path.definition)
                .filter(|path| !path.elevated)
            {
                match calculate_ground_path_construction_cells(
                    path.from,
                    path.to,
                    definition.width_cm,
                    definitions.topology_cell_size_cm(),
                    &index,
                ) {
                    Ok(cells) => {
                        let result = validate_path_construction_cells(
                            cells.clone(),
                            definition,
                            &index,
                            *grid,
                            definitions,
                            &terrain,
                            &terrain_assets,
                            &terrain_chunks,
                        );
                        match result {
                            Ok((_, cost)) => (cells, validity(Ok(cost))),
                            Err(reason) => (cells, validity(Err(reason))),
                        }
                    }
                    Err(reason) => (Vec::new(), validity(Err(reason))),
                }
            } else {
                let result = calculate_path_construction_plan(
                    &path,
                    &index,
                    *grid,
                    definitions,
                    &terrain,
                    &terrain_assets,
                    &terrain_chunks,
                );
                match result {
                    Ok((cells, cost)) => (cells, validity(Ok(cost))),
                    Err(reason) => (Vec::new(), validity(Err(reason))),
                }
            };
            let plan = PathDrawPlan {
                cells,
                validity: next,
            };
            if let Some(mut existing) = path_plan {
                existing.set_if_neq(plan);
            } else {
                commands.entity(entity).insert(plan);
            }
            next
        } else {
            continue;
        };
        if preview.validity != next {
            preview.validity = next;
        }
    }
}

pub(super) fn rotate_closed_fence_placement_heading(
    mut actions: MessageReader<openzt2_interface_state::input::input_types::ActionRequest>,
    tool: Res<ConstructionTool>,
    mut policy: ResMut<ConstructionPlacementPolicy>,
    mut previews: Query<&mut FencePreview>,
) {
    use openzt2_interface_state::input::input_types::GameAction;
    let turns = actions.read().fold(0_i32, |turns, request| {
        turns
            + match request.action {
                GameAction::RotateLeft | GameAction::RotateObjectLeft => -1,
                GameAction::RotateRight | GameAction::RotateObjectRight => 1,
                _ => 0,
            }
    });
    if !matches!(*tool, ConstructionTool::Fence(_))
        || policy.fence_shape == FencePlacementShape::Freeform
        || turns == 0
    {
        return;
    }
    let step = if policy.fence_shape == FencePlacementShape::Octagon {
        2
    } else {
        1
    };
    policy.fence_heading_eighth_turns =
        (i32::from(policy.fence_heading_eighth_turns) + turns * step).rem_euclid(8) as u8;
    for mut preview in &mut previews {
        preview.heading_eighth_turns = policy.fence_heading_eighth_turns;
    }
}
