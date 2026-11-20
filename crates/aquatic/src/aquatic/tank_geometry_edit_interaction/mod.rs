use bevy::picking::pointer::{PointerButton, PointerInput};
use bevy::prelude::*;
use openzt2_interface_state::input::input_types::MousePointer;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
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
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::{
    aquatic_simulation_types::{tank_habitat_seed_cell, Tank, TankGeometry},
    tank_geometry_edit_types::{TankGeometryEdit, TankGeometryGesture},
    tank_geometry_rules::{step_tank_geometry_edit, tank_geometry_limits},
    tank_height_bounds::TankHeightBoundsQueries,
};

/// Wall and floor edits are deferred until release so cancellation never
/// changes the live tank. Each frame of a held gesture moves the edited height
/// at the authored speed, clamped to the original's limits.
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
    grid: Res<TopologyGrid>,
    habitat_index: Res<HabitatIndex>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    cursors: Query<&ConstructionCursor>,
    roots: Query<Entity, With<WorldRoot>>,
    tanks: Query<(
        &Tank,
        &TankGeometry,
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
        ),
        Without<openzt2_zoo_state::aquatic::tank_geometry_edit_types::ScriptedTankGeometryEdit>,
    >,
    height_queries: TankHeightBoundsQueries,
    (mut cancellations, mut commits): (
        MessageReader<CancelConstruction>,
        MessageWriter<CommitConstruction>,
    ),
) {
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
                definitions.tank_edit_policy()?,
                tank_geometry_limits(definitions)?,
            ))
        });
    let Some((edit_policy, limits)) =
        rules.filter(|_| !cancelled && supported && !ui.over_ui && modal.0.is_none())
    else {
        for (entity, ..) in &previews {
            commands.entity(entity).despawn();
        }
        return;
    };
    if let Ok((entity, mut edit, mut gesture, mut preview)) = previews.single_mut() {
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
                step_tank_geometry_edit(&mut edit, delta, limits, gesture.bounds);
            }
        }
        preview.validity = if edit.before != edit.after && edit.after.is_valid() {
            PlacementValidity::Valid { cost: Money(0) }
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
    let Some((habitat, (tank, geometry, region, _, member))) = habitat_index
        .habitat_for_topology_cell(cell)
        .and_then(|habitat| tanks.get(habitat).ok().map(|facts| (habitat, facts)))
        .filter(|(_, (_, geometry, _, summary, member))| {
            member.root == root && !summary.boundary_is_breached && geometry.is_valid()
        })
    else {
        return;
    };
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
    commands.spawn((
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
}
