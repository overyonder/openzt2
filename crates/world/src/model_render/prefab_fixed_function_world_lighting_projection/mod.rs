use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentSpecularLightColor;

use openzt2_game_assets::material::runtime::effect_pass_gpu_data::D3d9FixedFunctionWorldLightContexts;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::D3d9FixedFunctionWorldLightRegisters;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::D3D9_MAX_ACTIVE_DIRECTIONAL_LIGHTS;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::FIXED_FUNCTION_WORLD_LIGHT_BUFFER;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT;
use openzt2_world_state::world_spawn::prefab_fixed_function_world_lighting_policy::PrefabFixedFunctionWorldLightingPolicy;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentTerrainAmbientLight;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;

use crate::model_render::prefab_render_layer_camera_matching::PrefabRenderLayers;
use openzt2_world_state::model_render::ModelExpanded;

#[derive(Component)]
pub(super) struct ProjectedPrefabFixedFunctionWorldLightingMaterial {
    source: Handle<EffectPassMaterial>,
    projected: AssetId<EffectPassMaterial>,
    policy: Option<PrefabFixedFunctionWorldLightingPolicy>,
    context: usize,
}

pub(super) fn project_prefab_fixed_function_world_lighting_policies(
    mut commands: Commands,
    mut changes: super::model_projection_changes::ModelProjectionChanges,
    branches: Query<
        Entity,
        Or<(
            Changed<PrefabFixedFunctionWorldLightingPolicy>,
            Changed<RenderLayers>,
            Changed<super::world_entity_model_suppression::SuppressedModelRenderLayers>,
        )>,
    >,
    mut removed_policy: RemovedComponents<PrefabFixedFunctionWorldLightingPolicy>,
    mut removed_layers: RemovedComponents<RenderLayers>,
    mut unsuppressed: RemovedComponents<
        super::world_entity_model_suppression::SuppressedModelRenderLayers,
    >,
    changed_materials: Query<Entity, Changed<MeshMaterial3d<EffectPassMaterial>>>,
    parents: Query<&ChildOf>,
    expanded: Query<(), With<ModelExpanded>>,
    policies: Query<&PrefabFixedFunctionWorldLightingPolicy>,
    mut inherited_layers: PrefabRenderLayers,
    primitives: Query<(
        &MeshMaterial3d<EffectPassMaterial>,
        Option<&ProjectedPrefabFixedFunctionWorldLightingMaterial>,
    )>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
) {
    changes.collect(
        branches
            .iter()
            .chain(removed_policy.read())
            .chain(removed_layers.read())
            .chain(unsuppressed.read()),
        changed_materials.iter(),
    );
    let mut missing = Vec::new();
    for entity in changes.candidates() {
        let Ok((current, previous)) = primitives.get(entity) else {
            continue;
        };
        let ancestors = || std::iter::once(entity).chain(parents.iter_ancestors(entity));
        let model = ancestors().find(|ancestor| expanded.contains(*ancestor));
        let policy = model.and_then(|root| {
            std::iter::once(root)
                .chain(parents.iter_ancestors(root))
                .find_map(|node| policies.get(node).ok().copied())
        });
        let context = if model.is_some() {
            inherited_layers
                .get(entity)
                .iter()
                .next()
                .filter(|index| *index < FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT)
                .unwrap_or_default()
        } else {
            0
        };
        let owned = previous.filter(|previous| previous.projected == current.id());
        if policy.is_none() && context == 0 {
            if let Some(previous) = owned {
                commands
                    .entity(entity)
                    .insert(MeshMaterial3d(previous.source.clone()));
            }
            if previous.is_some() {
                commands
                    .entity(entity)
                    .remove::<ProjectedPrefabFixedFunctionWorldLightingMaterial>();
            }
            continue;
        }
        if owned.is_some_and(|old| old.policy == policy && old.context == context) {
            continue;
        }
        let source = owned.map_or(&current.0, |previous| &previous.source);
        let Some(material) = materials.get(source) else {
            missing.push(entity);
            continue;
        };
        let Some(material) = material.clone_with_fixed_function_world_lighting_context(
            policy.map(|p| p.enabled),
            policy.is_some_and(|p| p.preserve_authored_material_lighting),
            context,
            &mut effect_uniform_buffers,
        ) else {
            continue;
        };
        let handle = materials.add(material);
        commands.entity(entity).insert((
            ProjectedPrefabFixedFunctionWorldLightingMaterial {
                source: source.clone(),
                projected: handle.id(),
                policy,
                context,
            },
            MeshMaterial3d(handle),
        ));
    }
    changes.retry_missing(&mut missing);
}

#[expect(clippy::needless_pass_by_value)]
pub(super) fn update_fixed_function_world_light_buffer(
    graphics: Res<GraphicsSettings>,
    ambient_light: Res<GlobalAmbientLight>,
    terrain_ambient: Res<EnvironmentTerrainAmbientLight>,
    camera_ambient_lights: Query<(&Camera, &AmbientLight, Option<&RenderLayers>), With<Camera3d>>,
    directional_lights: Query<(
        &DirectionalLight,
        &GlobalTransform,
        Option<&RenderLayers>,
        &ViewVisibility,
        Option<&EnvironmentSpecularLightColor>,
    )>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut previous_contexts: Local<Option<D3d9FixedFunctionWorldLightContexts>>,
) {
    let contexts = D3d9FixedFunctionWorldLightContexts {
        values: std::array::from_fn(|layer| {
            collect_fixed_function_world_light_registers_for_render_layers(
                &ambient_light,
                terrain_ambient.0,
                &camera_ambient_lights,
                &directional_lights,
                &RenderLayers::layer(layer),
                graphics.specular_effects,
            )
        }),
    };
    if previous_contexts.as_ref() == Some(&contexts) {
        return;
    }
    let Some(mut buffer) = buffers.get_mut(FIXED_FUNCTION_WORLD_LIGHT_BUFFER.id()) else {
        return;
    };
    contexts.write_shader_buffer(&mut buffer);
    *previous_contexts = Some(contexts);
}

fn collect_fixed_function_world_light_registers_for_render_layers(
    ambient_light: &GlobalAmbientLight,
    terrain_ambient: Option<Vec3>,
    camera_ambient_lights: &Query<(&Camera, &AmbientLight, Option<&RenderLayers>), With<Camera3d>>,
    directional_lights: &Query<(
        &DirectionalLight,
        &GlobalTransform,
        Option<&RenderLayers>,
        &ViewVisibility,
        Option<&EnvironmentSpecularLightColor>,
    )>,
    render_layers: &RenderLayers,
    specular_enabled: bool,
) -> D3d9FixedFunctionWorldLightRegisters {
    let mut registers = D3d9FixedFunctionWorldLightRegisters {
        specular_enabled: Vec4::new(f32::from(u8::from(specular_enabled)), 0.0, 0.0, 0.0),
        ..default()
    };
    let camera_ambient_light = camera_ambient_lights.iter().find_map(
        |(camera, camera_ambient_light, camera_render_layers)| {
            (camera.is_active
                && camera_render_layers
                    .unwrap_or_default()
                    .intersects(render_layers))
            .then_some(camera_ambient_light)
        },
    );
    let (ambient_color, ambient_brightness) = camera_ambient_light.map_or_else(
        || (ambient_light.color, ambient_light.brightness),
        |ambient| (ambient.color, ambient.brightness),
    );
    // The legacy shader combines authored colour bytes before converting its
    // final output to linear. Environment lights hold those bytes as their linear
    // channels, so reading them as sRGB would encode them a second time.
    registers.ambient_color_and_directional_light_count = terrain_ambient
        .filter(|_| render_layers.intersects(&RenderLayers::layer(TERRAIN_RENDER_LAYER)))
        .map_or_else(
            || Vec4::from_array(ambient_color.to_linear().to_f32_array()) * ambient_brightness,
            |ambient| ambient.extend(0.0),
        );
    let mut directional_light_count = 0;
    for (light, transform, light_layers, view_visibility, specular) in directional_lights {
        if !view_visibility.get()
            || !light_layers.unwrap_or_default().intersects(render_layers)
            || directional_light_count == D3D9_MAX_ACTIVE_DIRECTIONAL_LIGHTS
        {
            continue;
        }
        registers.directional_light_directions[directional_light_count] =
            Vec3::from(transform.back()).extend(0.0);
        registers.directional_light_colors[directional_light_count] =
            Vec4::from_array(light.color.to_linear().to_f32_array()) * light.illuminance;
        registers.directional_specular_colors[directional_light_count] = Vec4::from_array(
            specular
                .map_or(light.color, |color| color.0)
                .to_linear()
                .to_f32_array(),
        ) * light.illuminance;
        directional_light_count += 1;
    }
    registers.ambient_color_and_directional_light_count.w = directional_light_count as f32;
    registers
}
