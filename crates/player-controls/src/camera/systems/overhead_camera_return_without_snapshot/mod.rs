//! Recover the authored overhead camera when no temporary return pose exists.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

use super::super::camera_control_message_types::RestoreCameraMode;
use super::super::camera_runtime_state_types::CameraDefinition;
use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::OverheadRig;
use super::super::camera_runtime_state_types::ZooCamera;
use super::super::math::clamp_rig;
use super::super::math::overhead_pose;
use super::camera_definition_hydration::camera_runtime_components_from_authored_definition;
use super::camera_mode_transitions::apply_requested_transition;
use super::camera_mode_transitions::requested_duration_is_valid;
use crate::camera::camera_execution_types::CameraReturnState;
use crate::camera::camera_execution_types::RemoveReturnStateOnComplete;

pub(crate) fn restore_authored_overhead_camera_without_snapshot(
    mut requests: MessageReader<RestoreCameraMode>,
    mut commands: Commands,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    active_scenarios: Res<WorldScenarios>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    graphics: Res<GraphicsSettings>,
    roots: Query<(&SelectedWorldIdentity, &WorldTerrainHorizontalBounds)>,
    mut cameras: Query<
        (
            Entity,
            &Transform,
            &mut CameraMode,
            &mut CameraDefinition,
            &mut OverheadRig,
            &WorldMember,
        ),
        (With<ZooCamera>, Without<CameraReturnState>),
    >,
) {
    for request in requests.read() {
        if !requested_duration_is_valid(request.transition_seconds) {
            continue;
        }
        let (Some(scenarios), Some(definitions)) = (
            active_scenarios.get(&scenarios),
            active_definitions.get(&definitions),
        ) else {
            continue;
        };
        for (entity, transform, mut mode, mut selected, mut rig, member) in &mut cameras {
            // Repeated overhead selection does not reset an existing rig.
            // Snapshot-bearing cameras are excluded before exact restoration
            // consumes and removes their saved pose later in this schedule.
            if *mode == CameraMode::Overhead {
                continue;
            }
            let Ok((identity, world_bounds)) = roots.get(member.root) else {
                continue;
            };
            let Some(map) = scenarios.map(identity.map) else {
                continue;
            };
            let camera_id = AssetId(map.camera.0);
            let Some(definition) = definitions.find_camera(camera_id) else {
                continue;
            };
            let Some((_, tuning, mut authored_rig, bounds)) =
                camera_runtime_components_from_authored_definition(
                    definition,
                    world_bounds,
                    graphics.max_overhead_zoom,
                )
            else {
                continue;
            };
            authored_rig.focus = rig.focus;
            authored_rig.height_m = rig.height_m;
            authored_rig.yaw = rig.yaw;
            clamp_rig(&mut authored_rig, &tuning, &bounds);
            let target = overhead_pose(&authored_rig);
            *mode = CameraMode::Overhead;
            selected.0 = camera_id;
            *rig = authored_rig;
            commands
                .entity(entity)
                .remove::<RemoveReturnStateOnComplete>();
            apply_requested_transition(
                &mut commands,
                entity,
                transform,
                target,
                request.transition_seconds,
            );
        }
    }
}
