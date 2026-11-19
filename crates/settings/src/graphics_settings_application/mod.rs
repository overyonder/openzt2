use super::graphics_settings_presentation_types::SuppressedAuthoredTextShadow;
use bevy::{light::DirectionalLightShadowMap, prelude::*};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::graphics_settings_types::ReplaceGraphicsSettingsRequest;
use openzt2_game_assets::graphics_settings_types::ShadowDetail;

use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentLight;

use super::graphics_settings_validation::validate_proposed_graphics_settings;

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

pub(super) fn project_shadow_detail_to_environment_lights(
    settings: Res<GraphicsSettings>,
    shadow_map: ResMut<DirectionalLightShadowMap>,
    mut lights: Query<(&mut DirectionalLight, Ref<EnvironmentLight>)>,
) {
    let enabled = settings.shadow_detail != ShadowDetail::Off;
    if settings.is_changed() {
        let size = match settings.shadow_detail {
            ShadowDetail::Off | ShadowDetail::Medium => 2048,
            ShadowDetail::High => 3072,
            ShadowDetail::Ultra => 4096,
        };
        shadow_map
            .map_unchanged(|map| &mut map.size)
            .set_if_neq(size);
    }
    for (light, environment_light) in &mut lights {
        if settings.is_changed() || environment_light.is_added() || light.is_added() {
            light
                .map_unchanged(|light| &mut light.shadow_maps_enabled)
                .set_if_neq(enabled);
        }
    }
}

pub(super) fn project_multisample_count_to_zoo_cameras(
    mut commands: Commands,
    settings: Res<GraphicsSettings>,
    mut cameras: Query<(Entity, Ref<ZooCamera>, Option<&mut Msaa>)>,
) {
    // Clamp older profiles to the sample count supported on all devices.
    let samples = Msaa::from_samples(u32::from(settings.multisample_count.min(4)));
    for (entity, camera, current) in &mut cameras {
        if let Some(mut current) = current {
            if settings.is_changed() || camera.is_added() || current.is_added() {
                current.set_if_neq(samples);
            }
        } else {
            commands.entity(entity).insert(samples);
        }
    }
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
