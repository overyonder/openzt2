use bevy::picking::pointer::{PointerButton, PointerInput};
use bevy::prelude::*;
use openzt2_interface_state::input::input_types::MousePointer;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::construction::construction_interaction_types::CancelConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::TankEditKind;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_topology_cell;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::habitat::habitat_types::{HabitatCellOf, HabitatIndex};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::{
    aquatic_simulation_types::{tank_habitat_seed_cell, Tank, TankGeometry, TankWallTopHeight},
    tank_geometry_edit_execution::{write_tank_wall_height_images, LiveAppliedTankGeometry},
    tank_geometry_edit_types::{TankGeometryEdit, TankGeometryGesture},
    tank_geometry_rules::{
        step_tank_geometry_edit, tank_geometry_edit_reaches, tank_geometry_limits,
    },
    tank_height_bounds::TankHeightBoundsQueries,
};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_tank_geometry_gesture(
    mut commands: Commands,
    (tool, policy, time): (
        Res<ConstructionTool>,
        Res<ConstructionPlacementPolicy>,
        Res<Time>,
    ),
    (pointer, mut pointer_inputs, ui, modal): (
        MousePointer,
        MessageReader<PointerInput>,
        Res<UiPointerCapture>,
        Res<AuthoredModalInputCapture>,
    ),
    grid: Single<&TopologyGrid>,
    habitat_index: Single<&HabitatIndex>,
    cell_owners: Query<&HabitatCellOf>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    cursors: Query<&ConstructionCursor>,
    roots: Query<Entity, With<WorldRoot>>,
    mut tanks: Query<(
        &Tank,
        &mut TankGeometry,
        &HabitatRegion,
        &HabitatSummary,
        &WorldMember,
    )>,
    mut previews: Query<
        (
            Entity,
            &mut TankGeometryEdit,
            &mut TankGeometryGesture,
            &mut ConstructionPreview,
            Option<&LiveAppliedTankGeometry>,
        ),
        Without<openzt2_zoo_state::aquatic::tank_geometry_edit_types::ScriptedTankGeometryEdit>,
    >,
    mut wall_access: ParamSet<(
        TankHeightBoundsQueries,
        Query<(&PersistentId, &mut TankWallTopHeight)>,
    )>,
    (mut cancellations, mut commits): (
        MessageReader<CancelConstruction>,
        MessageWriter<CommitConstruction>,
    ),
) {
    let grid = grid.into_inner();
    let habitat_index = habitat_index.into_inner();
    let mut primary_just_pressed = false;
    let mut primary_just_released = false;
    for input in pointer_inputs
        .read()
        .filter(|input| input.pointer_id.is_mouse())
    {
        primary_just_pressed |= input.button_just_pressed(PointerButton::Primary);
        primary_just_released |= input.button_just_released(PointerButton::Primary);
    }
    let primary_pressed = pointer.primary_is_pressed();
    let cancelled = cancellations.read().count() != 0;
    let supported = *tool == ConstructionTool::TankEdit;
    let rules = active_definitions
        .get(&definitions)
        .and_then(|definitions| {
            Some((
                definitions,
                definitions.tank_edit_policy()?,
                tank_geometry_limits(definitions)?,
            ))
        });
    let Some((definitions, edit_policy, limits)) =
        rules.filter(|_| !cancelled && supported && !ui.over_ui && modal.0.is_none())
    else {
        for (entity, ..) in &previews {
            commands.entity(entity).despawn();
        }
        return;
    };
    if let Ok((entity, mut edit, mut gesture, mut preview, live)) = previews.single_mut() {
        if gesture.pending {
            return;
        }
        if gesture.kind != policy.tank_edit || (!primary_pressed && !primary_just_released) {
            commands.entity(entity).despawn();
            return;
        }
        if primary_pressed {
            let seconds = time.delta_secs();
            if seconds.is_finite() && seconds > 0.0 {
                let (direction, millimetres_per_second) = match gesture.kind {
                    TankEditKind::RaiseFloor => (1.0, edit_policy.floor_mm_per_second),
                    TankEditKind::LowerFloor => (-1.0, edit_policy.floor_mm_per_second),
                    TankEditKind::RaiseWall => (1.0, edit_policy.wall_mm_per_second),
                    TankEditKind::LowerWall => (-1.0, edit_policy.wall_mm_per_second),
                    TankEditKind::Fill => (1.0, edit_policy.water_mm_per_second),
                    TankEditKind::Drain => (-1.0, edit_policy.water_mm_per_second),
                };
                let delta = direction * millimetres_per_second as f32 / 1000.0 * seconds;
                if let Some(live) = live {
                    let tank = live.0;
                    let height_queries = wall_access.p0();
                    let Some((bounds, wall_heights, fences)) = (|| {
                        let (_, current, region, summary, member) = tanks.get(tank).ok()?;
                        if *current != edit.after || summary.boundary_is_breached {
                            return None;
                        }
                        let mut walls = height_queries.wall_height_images(tank, limits)?;
                        let bounds =
                            height_queries.tank_height_bounds(tank, region, limits, &walls)?;
                        height_queries.include_adjacent_tank_water_in_wall_minima(
                            &mut walls,
                            tank,
                            limits,
                            |entity| {
                                let (_, geometry, adjacent, _, world) = tanks.get(entity).ok()?;
                                (world.root == member.root
                                    && tank_habitat_seed_cell(adjacent) != Some(edit.seed_cell))
                                .then_some(geometry.water_height)
                            },
                        );
                        if walls.len() != edit.wall_heights.len()
                            || !walls.iter().zip(edit.wall_heights.iter()).all(
                                |(current, saved)| {
                                    current.id == saved.id && current.before == saved.after
                                },
                            )
                        {
                            return None;
                        }
                        let fences: Vec<_> = height_queries.boundary_fences(tank).collect();
                        Some((bounds, walls, fences))
                    })() else {
                        commands.entity(entity).despawn();
                        return;
                    };
                    let mut step = TankGeometryEdit {
                        seed_cell: edit.seed_cell,
                        kind: edit.kind,
                        definition: edit.definition,
                        before: edit.after,
                        after: edit.after,
                        wall_heights,
                    };
                    step_tank_geometry_edit(&mut step, delta, limits, bounds);
                    if tank_geometry_edit_reaches(&step, step.after, definitions, limits, bounds)
                        && write_tank_wall_height_images(
                            &fences,
                            &step,
                            true,
                            &mut wall_access.p1(),
                        )
                    {
                        if let Ok((_, mut geometry, ..)) = tanks.get_mut(tank) {
                            geometry.set_if_neq(step.after);
                            edit.after = step.after;
                            for (saved, step) in
                                edit.wall_heights.iter_mut().zip(step.wall_heights.iter())
                            {
                                saved.after = step.after;
                                saved.minimum = step.minimum;
                                saved.maximum = step.maximum;
                            }
                        }
                    }
                } else {
                    step_tank_geometry_edit(&mut edit, delta, limits, gesture.bounds);
                }
            }
        }
        preview.validity = if edit.before != edit.after && edit.after.is_valid() {
            PlacementValidity::Valid { cost: Money::ZERO }
        } else {
            PlacementValidity::Invalid(PlacementFailure::InvalidHabitat)
        };
        if primary_just_released {
            if matches!(preview.validity, PlacementValidity::Valid { .. }) {
                gesture.pending = true;
                commits.write(CommitConstruction { preview: entity });
            } else {
                commands.entity(entity).despawn();
            }
        }
        return;
    }
    if !previews.is_empty() || !primary_just_pressed || !primary_pressed {
        return;
    }
    let (Ok(cursor), Ok(root)) = (cursors.single(), roots.single()) else {
        return;
    };
    if !cursor.over_terrain
        || !cursor.world.is_finite()
        || !grid.origin.is_finite()
        || !grid.spacing_m.is_finite()
        || grid.spacing_m <= 0.0
    {
        return;
    }
    let cell = ((cursor.world.xz() - grid.origin) / grid.spacing_m)
        .floor()
        .as_ivec2();
    let Some((habitat, (tank, geometry, region, _, member))) =
        locate_habitat_at_topology_cell(habitat_index, &cell_owners, cell)
            .and_then(|habitat| tanks.get(habitat).ok().map(|facts| (habitat, facts)))
            .filter(|(_, (_, geometry, _, summary, member))| {
                member.root == root && !summary.boundary_is_breached && geometry.is_valid()
            })
    else {
        return;
    };
    let height_queries = wall_access.p0();
    let (Some(seed_cell), Some(mut wall_heights)) = (
        tank_habitat_seed_cell(region),
        height_queries.wall_height_images(habitat, limits),
    ) else {
        return;
    };
    let Some(bounds) = height_queries.tank_height_bounds(habitat, region, limits, &wall_heights)
    else {
        return;
    };
    height_queries.include_adjacent_tank_water_in_wall_minima(
        &mut wall_heights,
        habitat,
        limits,
        |entity| {
            let (_, geometry, adjacent, _, world) = tanks.get(entity).ok()?;
            (world.root == member.root && tank_habitat_seed_cell(adjacent) != Some(seed_cell))
                .then_some(geometry.water_height)
        },
    );
    let mut preview = commands.spawn((
        TankGeometryEdit {
            seed_cell,
            kind: policy.tank_edit,
            definition: tank.definition,
            before: *geometry,
            after: *geometry,
            wall_heights,
        },
        TankGeometryGesture {
            kind: policy.tank_edit,
            pending: false,
            bounds,
        },
        ConstructionPreview {
            definition: tank.definition,
            transform: Transform::from_translation(cursor.world),
            validity: PlacementValidity::Pending,
        },
        *member,
    ));
    if matches!(
        policy.tank_edit,
        TankEditKind::RaiseFloor
            | TankEditKind::LowerFloor
            | TankEditKind::RaiseWall
            | TankEditKind::LowerWall
    ) {
        preview.insert(LiveAppliedTankGeometry(habitat));
    }
}
