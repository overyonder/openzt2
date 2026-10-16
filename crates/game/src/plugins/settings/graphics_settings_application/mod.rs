use bevy::{light::DirectionalLightShadowMap, prelude::*};
use openzt2_game_assets::graphics_settings_types::{
    GraphicsSettings, ReplaceGraphicsSettingsRequest, ShadowDetail,
};

use crate::plugins::{
    camera::camera_runtime_state_types::ZooCamera,
    environment::environment_presentation_types::EnvironmentLight,
};

use super::{
    graphics_settings_presentation_types::SuppressedAuthoredTextShadow,
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
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    mut lights: Query<&mut DirectionalLight, With<EnvironmentLight>>,
) {
    let enabled = !matches!(settings.shadow_detail, ShadowDetail::Off);
    // ponytail: the source tiers publish no map sizes. Medium, the default
    // preset, keeps Bevy's 2048 default.
    let size = match settings.shadow_detail {
        ShadowDetail::Off | ShadowDetail::Medium => 2048,
        ShadowDetail::High => 3072,
        ShadowDetail::Ultra => 4096,
    };
    if shadow_map.size != size {
        shadow_map.size = size;
    }
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

pub(super) fn apply_accepted_ui_text_shadow_setting(
    mut commands: Commands,
    settings: Res<GraphicsSettings>,
    added: Query<(), Added<TextShadow>>,
    shown: Query<(Entity, &TextShadow)>,
    suppressed: Query<(Entity, &SuppressedAuthoredTextShadow)>,
) {
    if !settings.is_changed() && added.is_empty() {
        return;
    }
    if settings.ui_text_shadows {
        for (entity, shadow) in &suppressed {
            commands
                .entity(entity)
                .insert(shadow.0)
                .remove::<SuppressedAuthoredTextShadow>();
        }
    } else {
        for (entity, shadow) in &shown {
            commands
                .entity(entity)
                .insert(SuppressedAuthoredTextShadow(*shadow))
                .remove::<TextShadow>();
        }
    }
}
