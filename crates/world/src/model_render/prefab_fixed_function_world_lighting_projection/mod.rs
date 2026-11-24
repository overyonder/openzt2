use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentSpecularLightColor;

use openzt2_game_assets::material::runtime::effect_pass_gpu_data::D3d9FixedFunctionWorldLightContexts;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::D3d9FixedFunctionWorldLightRegisters;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::D3D9_MAX_ACTIVE_DIRECTIONAL_LIGHTS;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::FIXED_FUNCTION_WORLD_LIGHT_BUFFER;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentTerrainAmbientLight;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;

#[derive(Component, PartialEq)]
pub(super) struct FixedFunctionWorldLightRegisters(D3d9FixedFunctionWorldLightContexts);

impl Default for FixedFunctionWorldLightRegisters {
    fn default() -> Self {
        Self(D3d9FixedFunctionWorldLightContexts {
            values: [D3d9FixedFunctionWorldLightRegisters::default();
                FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT],
        })
    }
}

pub(super) fn upload_fixed_function_world_light_registers(
    registers: Single<Ref<FixedFunctionWorldLightRegisters>>,
    mut assets: MessageReader<AssetEvent<ShaderBuffer>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let recreated = assets.read().any(|event| {
        matches!(*event,
            AssetEvent::Added { id } if id == FIXED_FUNCTION_WORLD_LIGHT_BUFFER.id()
        )
    });
    if !registers.is_changed() && !recreated {
        return;
    }
    if let Some(mut buffer) = buffers.get_mut(FIXED_FUNCTION_WORLD_LIGHT_BUFFER.id()) {
        registers.0.write_shader_buffer(&mut buffer);
    }
}

pub(super) fn project_fixed_function_world_light_registers(
    graphics: Res<GraphicsSettings>,
    ambient_light: Res<GlobalAmbientLight>,
    terrain_ambient: Res<EnvironmentTerrainAmbientLight>,
    camera_ambient_lights: Query<(
        Ref<Camera>,
        Ref<Camera3d>,
        Ref<AmbientLight>,
        Option<Ref<RenderLayers>>,
    )>,
    directional_lights: Query<(
        Ref<DirectionalLight>,
        Ref<GlobalTransform>,
        Option<Ref<RenderLayers>>,
        Ref<ViewVisibility>,
        Option<Ref<EnvironmentSpecularLightColor>>,
    )>,
    light_entities: Query<(), With<DirectionalLight>>,
    mut removed_cameras: RemovedComponents<Camera3d>,
    mut removed_camera_components: RemovedComponents<Camera>,
    mut removed_ambient_lights: RemovedComponents<AmbientLight>,
    mut removed_directional_lights: RemovedComponents<DirectionalLight>,
    mut removed_light_transforms: RemovedComponents<GlobalTransform>,
    mut removed_light_visibility: RemovedComponents<ViewVisibility>,
    mut removed_layers: RemovedComponents<RenderLayers>,
    mut removed_specular_colors: RemovedComponents<EnvironmentSpecularLightColor>,
    mut published: Single<&mut FixedFunctionWorldLightRegisters>,
) {
    let removed_inputs = (removed_cameras.read().count() != 0)
        | (removed_camera_components.read().count() != 0)
        | (removed_ambient_lights.read().count() != 0)
        | (removed_directional_lights.read().count() != 0)
        | (removed_light_transforms
            .read()
            .filter(|entity| light_entities.contains(*entity))
            .count()
            != 0)
        | (removed_light_visibility
            .read()
            .filter(|entity| light_entities.contains(*entity))
            .count()
            != 0)
        | (removed_layers
            .read()
            .filter(|entity| {
                camera_ambient_lights.contains(*entity) || directional_lights.contains(*entity)
            })
            .count()
            != 0)
        | (removed_specular_colors.read().count() != 0);
    if !published.is_added()
        && !removed_inputs
        && !ambient_light.is_changed()
        && !terrain_ambient.is_changed()
        && !graphics.is_changed()
        && !camera_ambient_lights
            .iter()
            .any(|(camera, camera_3d, ambient, layers)| {
                camera.is_changed()
                    || camera_3d.is_added()
                    || ambient.is_changed()
                    || layers.is_some_and(|layers| layers.is_changed())
            })
        && !directional_lights
            .iter()
            .any(|(light, transform, layers, visibility, specular)| {
                light.is_changed()
                    || transform.is_changed()
                    || visibility.is_changed()
                    || layers.is_some_and(|layers| layers.is_changed())
                    || specular.is_some_and(|color| color.is_changed())
            })
    {
        return;
    }
    let default_ambient =
        Vec4::from_array(ambient_light.color.to_linear().to_f32_array()) * ambient_light.brightness;
    let specular_enabled = f32::from(u8::from(graphics.specular_effects));
    let mut contexts = D3d9FixedFunctionWorldLightContexts {
        values: [D3d9FixedFunctionWorldLightRegisters {
            ambient_color_and_directional_light_count: default_ambient,
            specular_enabled: Vec4::new(specular_enabled, 0.0, 0.0, 0.0),
            ..default()
        }; FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT],
    };
    let mut ambient_overrides = [false; FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT];
    for (camera, _, ambient, layers) in &camera_ambient_lights {
        if !camera.is_active {
            continue;
        }
        let ambient =
            Vec4::from_array(ambient.color.to_linear().to_f32_array()) * ambient.brightness;
        for layer in layers.as_deref().unwrap_or_default().iter() {
            let Some(applied) = ambient_overrides.get_mut(layer) else {
                continue;
            };
            if !*applied {
                contexts.values[layer].ambient_color_and_directional_light_count = ambient;
                *applied = true;
            }
        }
    }
    // Environment ambient already carries the authored channels; the shader
    // combines them before converting its final output to linear.
    if let Some(ambient) = terrain_ambient.0 {
        contexts.values[TERRAIN_RENDER_LAYER].ambient_color_and_directional_light_count =
            ambient.extend(0.0);
    }

    let mut directional_counts = [0; FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT];
    for (light, transform, layers, visibility, specular) in &directional_lights {
        if !visibility.get() {
            continue;
        }
        let direction = Vec3::from(transform.back()).extend(0.0);
        let color = Vec4::from_array(light.color.to_linear().to_f32_array()) * light.illuminance;
        let specular = Vec4::from_array(
            specular
                .map_or(light.color, |color| color.0)
                .to_linear()
                .to_f32_array(),
        ) * light.illuminance;
        for layer in layers.as_deref().unwrap_or_default().iter() {
            let Some(count) = directional_counts.get_mut(layer) else {
                continue;
            };
            if *count == D3D9_MAX_ACTIVE_DIRECTIONAL_LIGHTS {
                continue;
            }
            let registers = &mut contexts.values[layer];
            registers.directional_light_directions[*count] = direction;
            registers.directional_light_colors[*count] = color;
            registers.directional_specular_colors[*count] = specular;
            *count += 1;
        }
    }
    for (registers, count) in contexts.values.iter_mut().zip(directional_counts) {
        registers.ambient_color_and_directional_light_count.w = count as f32;
    }
    published.set_if_neq(FixedFunctionWorldLightRegisters(contexts));
}
