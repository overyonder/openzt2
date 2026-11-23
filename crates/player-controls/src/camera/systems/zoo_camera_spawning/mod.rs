use bevy::{
    camera::{visibility::RenderLayers, Exposure},
    prelude::*,
    ui_render::UiAntiAlias,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_world_state::model_render::authored_model_material_pass_projection::PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

use super::super::camera_runtime_state_types::CameraDefinition;
use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::ZooCamera;
use super::super::math::clamp_rig;
use super::super::math::overhead_pose;
use super::camera_definition_hydration::camera_runtime_components_from_authored_definition;
use crate::camera::camera_execution_types::CameraIntent;
use crate::camera::camera_execution_types::CameraSpawnPending;
use crate::camera::camera_execution_types::CameraSpawnPendingReason;

#[derive(Component)]
#[relationship(relationship_target = WorldZooCameras)]
pub(crate) struct ZooCameraForWorld(Entity);

#[derive(Component)]
#[relationship_target(relationship = ZooCameraForWorld)]
pub(crate) struct WorldZooCameras(Vec<Entity>);

/// Spawns the selected camera once world bounds and its definition have loaded.
pub(crate) fn spawn_zoo_camera(
    mut commands: Commands,
    active_scenarios: Res<WorldScenarios>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    graphics: Res<GraphicsSettings>,
    mut roots: Query<
        (
            Entity,
            Ref<SelectedWorldIdentity>,
            Ref<WorldTerrainHorizontalBounds>,
            Option<&mut CameraSpawnPending>,
            Option<Ref<SelectedWorldDocumentHandles>>,
        ),
        (With<WorldRoot>, Without<WorldZooCameras>),
    >,
    mut released_camera_worlds: RemovedComponents<WorldZooCameras>,
) {
    let released: bevy::ecs::entity::EntityHashSet = released_camera_worlds.read().collect();
    if roots.is_empty() {
        return;
    }
    let assets_changed = active_scenarios.is_changed()
        || scenarios.is_changed()
        || definitions.is_changed()
        || active_definitions.is_changed()
        || graphics.is_changed();
    let Some(scenario_catalogue) = active_scenarios.get(&scenarios) else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (root, selection, world_bounds, pending, documents) in &mut roots {
        if !assets_changed
            && !selection.is_changed()
            && !world_bounds.is_changed()
            && !documents.as_ref().is_some_and(DetectChanges::is_changed)
            && !released.contains(&root)
        {
            continue;
        }
        let documents = documents.as_deref();
        let scoped_catalogue = match documents {
            Some(documents) => scenario_catalogue
                .with_document(&documents.map)
                .and_then(|view| view.with_document(&documents.start)),
            None => Some(scenario_catalogue),
        };
        let Some(scoped_catalogue) = scoped_catalogue else {
            continue;
        };
        let Some(map) = scoped_catalogue.map(selection.map) else {
            set_zoo_camera_spawn_pending_reason(
                &mut commands,
                root,
                pending,
                CameraSpawnPendingReason::MissingMap,
            );
            continue;
        };
        let start = match documents {
            Some(documents) => scenarios
                .get(&documents.start)
                .and_then(|document| document.document.find_starting_zoo(selection.start)),
            None => scenarios
                .iter()
                .find_map(|(_, document)| document.document.find_starting_zoo(selection.start)),
        };
        let Some(start) = start else {
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
                &world_bounds,
                &graphics,
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
            // World sounds are heard from the camera, not the world origin.
            bevy::audio::SpatialListener::default(),
            RenderLayers::from_layers(&[
                0,
                TERRAIN_RENDER_LAYER,
                TERRAIN_WATER_RENDER_LAYER,
                PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER,
            ]),
            (
                CameraDefinition(camera_id),
                crate::camera::camera_execution_types::MapOverheadCameraDefinition(camera_id),
            ),
            CameraMode::Overhead,
            rig,
            tuning,
            bounds,
            CameraIntent::default(),
            (WorldMember { root }, ZooCameraForWorld(root)),
        ));
        if pending.is_some() {
            commands.entity(root).remove::<CameraSpawnPending>();
        }
    }
}

fn set_zoo_camera_spawn_pending_reason(
    commands: &mut Commands,
    root: Entity,
    current: Option<Mut<CameraSpawnPending>>,
    reason: CameraSpawnPendingReason,
) {
    if current.as_ref().is_some_and(|current| current.0 == reason) {
        return;
    }
    warn!(
        ?root,
        ?reason,
        "zoo camera hydration is waiting on a native dependency"
    );
    if let Some(mut current) = current {
        current.set_if_neq(CameraSpawnPending(reason));
    } else {
        commands.entity(root).insert(CameraSpawnPending(reason));
    }
}
