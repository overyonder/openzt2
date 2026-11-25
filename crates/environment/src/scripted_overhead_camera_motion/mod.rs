use bevy::prelude::*;
use openzt2_game_assets::{
    graphics_settings_types::{GraphicsSettings, ThreeLevelGraphicsDetail},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_interface_state::camera::{
    camera_control_message_types::{ScriptedOverheadCameraMotion, ScriptedOverheadCameraOperation},
    camera_runtime_state_types::{CameraDefinition, CameraMode, OverheadRig, ZooCamera},
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

pub(super) fn apply_scripted_overhead_camera_motion(
    mut motions: MessageReader<ScriptedOverheadCameraMotion>,
    graphics: Res<GraphicsSettings>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut cameras: Query<
        (
            &Camera,
            &CameraMode,
            &WorldMember,
            &CameraDefinition,
            &mut OverheadRig,
        ),
        With<ZooCamera>,
    >,
) {
    if motions.is_empty() {
        return;
    }
    let definitions = definitions.get(&assets);
    for command in motions.read() {
        let Some((_, _, _, definition, mut rig)) =
            cameras.iter_mut().find(|(camera, mode, member, ..)| {
                camera.is_active && **mode == CameraMode::Overhead && member.root == command.root
            })
        else {
            continue;
        };
        match command.operation {
            ScriptedOverheadCameraOperation::Pan { world } if world.is_finite() => {
                rig.reborrow()
                    .map_unchanged(|rig| &mut rig.focus)
                    .set_if_neq(world.xz());
                rig.reborrow()
                    .map_unchanged(|rig| &mut rig.height_m)
                    .set_if_neq(world.y);
            }
            ScriptedOverheadCameraOperation::Zoom { distance }
                if distance.is_finite() && distance > 0.0 =>
            {
                let Some(camera) = definitions
                    .as_ref()
                    .and_then(|definitions| definitions.find_camera(definition.0))
                else {
                    continue;
                };
                let quality = match graphics.max_overhead_zoom {
                    ThreeLevelGraphicsDetail::Low => 0,
                    ThreeLevelGraphicsDetail::Medium => 1,
                    ThreeLevelGraphicsDetail::High => 2,
                };
                let distance =
                    distance.clamp(camera.minimum_zoom_m, camera.maximum_zoom_m[quality]);
                rig.reborrow()
                    .map_unchanged(|rig| &mut rig.distance)
                    .set_if_neq(distance);
                rig.map_unchanged(|rig| &mut rig.minimum_zoom_offset_m)
                    .set_if_neq(0.0);
            }
            _ => {}
        }
    }
}
