//! The wind, light rig and fog distances every programmable effect reads,
//! expressed in the original game's z-up world.

use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::{
    EffectEnvironmentRegisters, EFFECT_ENVIRONMENT_REGISTER_BUFFER,
};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentLight;
use openzt2_zoo_state::environment::environment_state_types::{
    AuthoredWindShaderPresentationState, WorldEnvironment,
};

/// The shipped `lightFromRig` lights by `dot(-direction, normal)`: each
/// direction is the way its light travels, unlike the fixed-function registers.
pub(super) fn light_rig<'a>(
    ambient_light: &GlobalAmbientLight,
    lights: impl IntoIterator<
        Item = (
            &'a DirectionalLight,
            &'a GlobalTransform,
            bool,
            &'a EnvironmentLight,
        ),
    >,
) -> [Vec4; 7] {
    let mut rig = [Vec4::ZERO; 7];
    for (light, transform, visible, authored_light) in lights {
        let Some(index) = authored_light.kind_rank.checked_sub(1).map(usize::from) else {
            continue;
        };
        if !visible || index >= 3 {
            continue;
        }
        let mut color = Vec4::from_array(light.color.to_linear().to_f32_array());
        color *= light.illuminance / light_consts::lux::DIRECT_SUNLIGHT;
        color.w = 1.0;
        rig[index] = color;
        rig[index + 3] = Vec3::from(transform.forward()).extend(1.0);
    }
    rig[6] =
        Vec4::from_array(ambient_light.color.to_linear().to_f32_array()) * ambient_light.brightness;
    rig[6].w = 1.0;
    rig
}

fn source_from_world(direction: Vec4) -> Vec4 {
    Vec4::new(direction.x, -direction.z, direction.y, direction.w)
}

#[expect(clippy::needless_pass_by_value)]
pub(super) fn write_effect_environment_registers(
    ambient_light: Res<GlobalAmbientLight>,
    directional_lights: Query<(
        &DirectionalLight,
        &GlobalTransform,
        &ViewVisibility,
        &EnvironmentLight,
    )>,
    wind: Single<&AuthoredWindShaderPresentationState, With<WorldEnvironment>>,
    fogs: Query<&DistanceFog, With<ZooCamera>>,
    mut previous: Local<Option<EffectEnvironmentRegisters>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let mut light_rig = light_rig(
        &ambient_light,
        directional_lights
            .iter()
            .map(|(light, transform, visibility, authored)| {
                (light, transform, visibility.get(), authored)
            }),
    );
    for direction in &mut light_rig[3..6] {
        *direction = source_from_world(*direction);
    }
    let fog_distances = fogs
        .iter()
        .find_map(|fog| match fog.falloff {
            FogFalloff::Linear { start, end } => {
                Some(Vec4::new(start, (end - start).recip(), end, 0.0))
            }
            _ => None,
        })
        .unwrap_or_default();
    let registers = EffectEnvironmentRegisters {
        wind: wind.shader_registers(),
        light_rig,
        fog_distances,
    };
    if previous.as_ref() == Some(&registers) {
        return;
    }
    let Some(mut buffer) = buffers.get_mut(EFFECT_ENVIRONMENT_REGISTER_BUFFER.id()) else {
        return;
    };
    registers.write_shader_buffer(&mut buffer);
    *previous = Some(registers);
}
