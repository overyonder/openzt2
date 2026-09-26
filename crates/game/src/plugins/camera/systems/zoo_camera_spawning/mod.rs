use bevy::{
    camera::{visibility::RenderLayers, Exposure},
    prelude::*,
    ui_render::UiAntiAlias,
};
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::plugins::model_render::authored_model_material_pass_projection::PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER;
use crate::plugins::settings::graphics_settings_types::GraphicsSettings;
use crate::plugins::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use crate::plugins::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;
use crate::plugins::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

use super::{
    super::{
        camera_runtime_state_types::{
            CameraDefinition, CameraIntent, CameraMode, CameraSpawnPending,
            CameraSpawnPendingReason, ZooCamera,
        },
        math::{clamp_rig, overhead_pose},
    },
    camera_definition_hydration::camera_runtime_components_from_authored_definition,
};

/// Spawns the selected camera once world bounds and its definition have loaded.
pub(in crate::plugins::camera) fn spawn_zoo_camera(
    mut commands: Commands,
    active_scenarios: Res<WorldScenarios>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    graphics: Res<GraphicsSettings>,
    roots: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &WorldTerrainHorizontalBounds,
            Option<&CameraSpawnPending>,
        ),
        With<WorldRoot>,
    >,
    cameras: Query<&WorldMember, With<ZooCamera>>,
) {
    let Some(scenario_catalogue) = active_scenarios.get(&scenarios) else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (root, selection, world_bounds, pending) in &roots {
        if cameras.iter().any(|member| member.root == root) {
            if pending.is_some() {
                commands.entity(root).remove::<CameraSpawnPending>();
            }
            continue;
        }

        let Some(map) = scenario_catalogue.map(selection.map) else {
            set_zoo_camera_spawn_pending_reason(
                &mut commands,
                root,
                pending,
                CameraSpawnPendingReason::MissingMap,
            );
            continue;
        };
        let Some(start) = scenarios
            .iter()
            .find_map(|(_, document)| document.document.find_starting_zoo(selection.start))
        else {
            set_zoo_camera_spawn_pending_reason(
                &mut commands,
                root,
                pending,
                CameraSpawnPendingReason::MissingStartingZoo,
            );
            continue;
        };
        let camera_id = AssetId(map.camera.0);
        let Some(definition) = definitions.find_camera(camera_id) else {
            set_zoo_camera_spawn_pending_reason(
                &mut commands,
                root,
                pending,
                CameraSpawnPendingReason::MissingCameraDefinition,
            );
            continue;
        };
        let Some((projection, tuning, mut rig, bounds)) =
            camera_runtime_components_from_authored_definition(
                definition,
                world_bounds,
                graphics.max_overhead_zoom,
            )
        else {
            set_zoo_camera_spawn_pending_reason(
                &mut commands,
                root,
                pending,
                CameraSpawnPendingReason::InvalidCameraDefinition,
            );
            continue;
        };

        rig.focus = Vec2::new(
            f32::from(start.camera_position_m[0]),
            f32::from(start.camera_position_m[2]),
        );
        rig.height_m = f32::from(start.camera_position_m[1]);
        // `camPitch` is the saved rotation of the overhead rig's root. The
        // camera node keeps the authored `pitchRotate` from overheadcam.xml;
        // a saved value of zero therefore means the normal overhead angle,
        // not a camera placed horizontally along the ground.
        rig.pitch += f32::from(start.camera_rotation_radians[0]);
        // Keep the saved source-Z angle. `overhead_pose` applies the proper
        // source-Z-up to Bevy-Y-up basis while flattening the native node chain.
        rig.yaw = f32::from(start.camera_rotation_radians[1]);
        clamp_rig(&mut rig, &tuning, &bounds);
        let transform = overhead_pose(&rig);
        commands.spawn((
            Camera3d::default(),
            // Authored UI is composed from pixel-aligned rectangular images.
            // Shape-edge antialiasing creates fractional coverage between
            // adjacent tiles; text retains its independent font smoothing.
            UiAntiAlias::Off,
            // Environment lights are projected into Bevy's physical lux
            // units. Match that contract with an outdoor camera exposure;
            // Bevy's Blender-calibrated default clips the authored daytime
            // palette and collapses terrain and model contrast.
            Exposure::SUNLIGHT,
            projection,
            transform,
            ZooCamera,
            RenderLayers::from_layers(&[
                0,
                TERRAIN_RENDER_LAYER,
                TERRAIN_WATER_RENDER_LAYER,
                PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER,
            ]),
            CameraDefinition(camera_id),
            CameraMode::Overhead,
            rig,
            tuning,
            bounds,
            CameraIntent::default(),
            WorldMember { root },
        ));
        if pending.is_some() {
            commands.entity(root).remove::<CameraSpawnPending>();
        }
    }
}

fn set_zoo_camera_spawn_pending_reason(
    commands: &mut Commands,
    root: Entity,
    current: Option<&CameraSpawnPending>,
    reason: CameraSpawnPendingReason,
) {
    if current.is_none_or(|current| current.0 != reason) {
        warn!(
            ?root,
            ?reason,
            "zoo camera hydration is waiting on a native dependency"
        );
        commands.entity(root).insert(CameraSpawnPending(reason));
    }
}
