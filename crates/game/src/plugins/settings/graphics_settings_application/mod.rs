use bevy::prelude::*;

use crate::plugins::{
    camera::camera_runtime_state_types::ZooCamera,
    environment::environment_presentation_types::EnvironmentLight,
};

use super::{
    graphics_settings_types::{GraphicsSettings, ReplaceGraphicsSettingsRequest, ShadowDetail},
    graphics_settings_validation::validate_proposed_graphics_settings,
};

pub(super) fn validate_and_accept_requested_graphics_settings(
    mut requests: MessageReader<ReplaceGraphicsSettingsRequest>,
    mut current: ResMut<GraphicsSettings>,
) {
    for request in requests.read() {
        if validate_proposed_graphics_settings(request.0).is_err() {
            continue;
        }
        if *current != request.0 {
            *current = request.0;
        }
    }
}

pub(super) fn apply_accepted_shadow_detail_to_environment_lights(
    settings: Res<GraphicsSettings>,
    mut lights: Query<&mut DirectionalLight, With<EnvironmentLight>>,
) {
    let enabled = !matches!(settings.shadow_detail, ShadowDetail::Off);
    for mut light in &mut lights {
        light.shadow_maps_enabled = enabled;
    }
}

pub(super) fn apply_accepted_multisample_count_to_zoo_cameras(
    mut commands: Commands,
    settings: Res<GraphicsSettings>,
    cameras: Query<Entity, With<ZooCamera>>,
) {
    let msaa = Msaa::from_samples(u32::from(settings.multisample_count));
    for camera in &cameras {
        commands.entity(camera).insert(msaa);
    }
}

pub(super) fn graphics_settings_changed_or_render_target_was_added(
    settings: Res<GraphicsSettings>,
    cameras: Query<(), Added<ZooCamera>>,
    lights: Query<(), Added<EnvironmentLight>>,
) -> bool {
    settings.is_changed() || !cameras.is_empty() || !lights.is_empty()
}
