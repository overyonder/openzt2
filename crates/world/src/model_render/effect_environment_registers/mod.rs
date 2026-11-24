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

#[derive(Component, Default, PartialEq)]
pub(super) struct ProgrammableEnvironmentRegisters(EffectEnvironmentRegisters);

pub(super) fn upload_programmable_environment_registers(
    registers: Single<Ref<ProgrammableEnvironmentRegisters>>,
    mut assets: MessageReader<AssetEvent<ShaderBuffer>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let recreated = assets.read().any(|event| {
        matches!(*event,
            AssetEvent::Added { id } if id == EFFECT_ENVIRONMENT_REGISTER_BUFFER.id()
        )
    });
    if !registers.is_changed() && !recreated {
        return;
    }
    if let Some(mut buffer) = buffers.get_mut(EFFECT_ENVIRONMENT_REGISTER_BUFFER.id()) {
        registers.0.write_shader_buffer(&mut buffer);
    }
}

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

pub(super) fn project_effect_environment_registers(
    ambient_light: Res<GlobalAmbientLight>,
    directional_lights: Query<(
        Ref<DirectionalLight>,
        Ref<GlobalTransform>,
        Ref<ViewVisibility>,
        Ref<EnvironmentLight>,
    )>,
    authored_lights: Query<(), With<EnvironmentLight>>,
    wind: Option<
        Single<(
            Ref<AuthoredWindShaderPresentationState>,
            Ref<WorldEnvironment>,
        )>,
    >,
    fogs: Query<(Ref<DistanceFog>, Ref<ZooCamera>)>,
    cameras: Query<(), With<ZooCamera>>,
    mut removed_lights: RemovedComponents<EnvironmentLight>,
    mut removed_directional_lights: RemovedComponents<DirectionalLight>,
    mut removed_light_transforms: RemovedComponents<GlobalTransform>,
    mut removed_light_visibility: RemovedComponents<ViewVisibility>,
    mut removed_fogs: RemovedComponents<DistanceFog>,
    mut removed_cameras: RemovedComponents<ZooCamera>,
    mut removed_winds: RemovedComponents<AuthoredWindShaderPresentationState>,
    mut removed_environments: RemovedComponents<WorldEnvironment>,
    mut published: Single<&mut ProgrammableEnvironmentRegisters>,
) {
    let removed_light_inputs = (removed_lights.read().count() != 0)
        | (removed_directional_lights
            .read()
            .filter(|entity| authored_lights.contains(*entity))
            .count()
            != 0)
        | (removed_light_transforms
            .read()
            .filter(|entity| authored_lights.contains(*entity))
            .count()
            != 0)
        | (removed_light_visibility
            .read()
            .filter(|entity| authored_lights.contains(*entity))
            .count()
            != 0);
    let removed_fog_inputs = (removed_fogs
        .read()
        .filter(|entity| cameras.contains(*entity))
        .count()
        != 0)
        | (removed_cameras.read().count() != 0);
    let removed_wind_inputs =
        (removed_winds.read().count() != 0) | (removed_environments.read().count() != 0);

    let mut registers = published.0;
    if published.is_added()
        || removed_light_inputs
        || ambient_light.is_changed()
        || directional_lights
            .iter()
            .any(|(light, transform, visibility, authored)| {
                light.is_changed()
                    || transform.is_changed()
                    || visibility.is_changed()
                    || authored.is_changed()
            })
    {
        registers.light_rig = light_rig(
            &ambient_light,
            directional_lights
                .iter()
                .map(|(light, transform, visibility, authored)| {
                    (
                        light.into_inner(),
                        transform.into_inner(),
                        visibility.get(),
                        authored.into_inner(),
                    )
                }),
        );
        for direction in &mut registers.light_rig[3..6] {
            *direction = source_from_world(*direction);
        }
    }
    if published.is_added()
        || removed_fog_inputs
        || fogs
            .iter()
            .any(|(fog, camera)| fog.is_changed() || camera.is_added())
    {
        registers.fog_distances = fogs
            .iter()
            .find_map(|(fog, _)| match fog.falloff {
                FogFalloff::Linear { start, end } => {
                    Some(Vec4::new(start, (end - start).recip(), end, 0.0))
                }
                _ => None,
            })
            .unwrap_or_default();
    }
    if let Some(wind) = wind {
        let (wind, environment) = wind.into_inner();
        if published.is_added()
            || removed_wind_inputs
            || wind.is_changed()
            || environment.is_added()
        {
            registers.wind = wind.shader_registers();
        }
    } else {
        registers.wind = [Vec4::ZERO; 7];
    }
    published.set_if_neq(ProgrammableEnvironmentRegisters(registers));
}
