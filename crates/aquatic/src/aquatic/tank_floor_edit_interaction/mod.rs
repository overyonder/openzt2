use bevy::prelude::*;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_interface_state::input::input_types::PrimaryPointerInputState;
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
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::{
    aquatic_simulation_types::{Tank, TankGeometry, TankSurfaceHydrated},
    tank_floor_edit_types::{TankFloorEdit, TankFloorGesture},
    tank_floor_edit_validation::tank_floor_edit_matches_authored_depth_bounds,
};

/// Floor edits are deferred until release so cancellation never changes the live surface.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_tank_floor_gesture(
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    policy: Res<ConstructionPlacementPolicy>,
    time: Res<Time>,
    pointer: Res<PrimaryPointerInputState>,
    ui: Res<UiPointerCapture>,
    modal: Res<AuthoredModalInputCapture>,
    grid: Res<TopologyGrid>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    cursors: Query<&ConstructionCursor>,
    roots: Query<Entity, With<WorldRoot>>,
    tanks: Query<
        (
            Entity,
            &Tank,
            &TankGeometry,
            &HabitatRegion,
            &HabitatSummary,
            &WorldMember,
        ),
        With<TankSurfaceHydrated>,
    >,
    mut previews: Query<(
        Entity,
        &mut TankFloorEdit,
        &mut TankFloorGesture,
        &mut ConstructionPreview,
    )>,
    mut cancellations: MessageReader<CancelConstruction>,
    mut commits: MessageWriter<CommitConstruction>,
) {
    let cancelled = cancellations.read().count() != 0;
    let supported = *tool == ConstructionTool::TankEdit
        && matches!(
            policy.tank_edit,
            TankEditKind::RaiseFloor | TankEditKind::LowerFloor
        );
    if cancelled || !supported || ui.over_ui || modal.0.is_some() {
        for (entity, _, _, _) in &previews {
            commands.entity(entity).despawn();
        }
        return;
    }
    let Some((definitions, edit_policy)) =
        active_definitions
            .get(&definitions)
            .and_then(|definitions| {
                definitions
                    .tank_edit_policy()
                    .map(|policy| (definitions, policy))
            })
    else {
        for (entity, _, _, _) in &previews {
            commands.entity(entity).despawn();
        }
        return;
    };
    if let Ok((entity, mut edit, mut gesture, mut preview)) = previews.single_mut() {
        if gesture.pending {
            return;
        }
        if gesture.kind != policy.tank_edit || (!pointer.pressed && !pointer.just_released) {
            commands.entity(entity).despawn();
            return;
        }
        if pointer.pressed {
            let seconds = time.delta_secs();
            if seconds.is_finite() && seconds > 0.0 {
                let direction = if gesture.kind == TankEditKind::RaiseFloor {
                    1.0
                } else {
                    -1.0
                };
                edit.after.floor_height +=
                    direction * edit_policy.floor_mm_per_second as f32 / 1000.0 * seconds;
                edit.after.volume = edit.after.area * edit.after.depth();
            }
        }
        preview.validity = if edit.before != edit.after
            && tank_floor_edit_matches_authored_depth_bounds(&edit, edit.after, definitions)
        {
            PlacementValidity::Valid { cost: Money(0) }
        } else {
            PlacementValidity::Invalid(PlacementFailure::InvalidHabitat)
        };
        if pointer.just_released {
            if matches!(preview.validity, PlacementValidity::Valid { .. }) {
                gesture.pending = true;
                commits.write(CommitConstruction { preview: entity });
            } else {
                commands.entity(entity).despawn();
            }
        }
        return;
    }
    if !previews.is_empty() || !pointer.just_pressed || !pointer.pressed {
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
    let mut candidates = tanks
        .iter()
        .filter(|(_, _, geometry, region, summary, member)| {
            member.root == root
                && !summary.boundary_is_breached
                && geometry.is_valid()
                && region.topology_cells.contains(&cell)
        });
    let Some((entity, tank, geometry, _, _, member)) = candidates.next() else {
        return;
    };
    if candidates.next().is_some() {
        return;
    }
    let edit = TankFloorEdit {
        tank: entity,
        definition: tank.definition,
        before: *geometry,
        after: *geometry,
    };
    if !tank_floor_edit_matches_authored_depth_bounds(&edit, *geometry, definitions) {
        return;
    }
    commands.spawn((
        edit,
        TankFloorGesture {
            kind: policy.tank_edit,
            pending: false,
        },
        ConstructionPreview {
            definition: tank.definition,
            transform: Transform::from_translation(cursor.world),
            validity: PlacementValidity::Pending,
        },
        *member,
    ));
}
