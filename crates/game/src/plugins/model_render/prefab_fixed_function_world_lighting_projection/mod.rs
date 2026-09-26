use bevy::{camera::visibility::RenderLayers, prelude::*, render::storage::ShaderBuffer};

use crate::{
    assets::material::runtime::effect_pass_gpu_data::{
        D3d9FixedFunctionWorldLightContexts, D3d9FixedFunctionWorldLightRegisters,
        EffectPassMaterial, D3D9_MAX_ACTIVE_DIRECTIONAL_LIGHTS, FIXED_FUNCTION_WORLD_LIGHT_BUFFER,
        FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT,
    },
    plugins::world_spawn::prefab_fixed_function_world_lighting_policy::PrefabFixedFunctionWorldLightingPolicy,
};

use super::{
    authored_model_material_pass_projection::{
        EffectPassMaterialRenderViewProjection, EffectPassMaterialSourceRenderLayers,
    },
    prefab_render_layer_camera_matching::render_layers_for_prefab_entity_and_ancestor_chain,
    ModelExpanded,
};

#[derive(Component)]
pub(super) struct ProjectedPrefabFixedFunctionWorldLightingMaterial(Handle<EffectPassMaterial>);

pub(super) fn project_prefab_fixed_function_world_lighting_policies(
    mut commands: Commands,
    roots: Query<(Entity, Ref<ModelExpanded>)>,
    changed_lighting_policies: Query<(), Changed<PrefabFixedFunctionWorldLightingPolicy>>,
    changed_render_layers: Query<(), Changed<RenderLayers>>,
    changed_effect_materials: Query<(), Changed<MeshMaterial3d<EffectPassMaterial>>>,
    hierarchy: Query<(
        Option<&PrefabFixedFunctionWorldLightingPolicy>,
        Option<&ChildOf>,
    )>,
    render_layer_hierarchy: Query<(Option<&ChildOf>, Option<&RenderLayers>)>,
    children: Query<&Children>,
    primitives: Query<
        (
            Entity,
            &MeshMaterial3d<EffectPassMaterial>,
            Option<&ProjectedPrefabFixedFunctionWorldLightingMaterial>,
            Option<&EffectPassMaterialSourceRenderLayers>,
        ),
        Without<EffectPassMaterialRenderViewProjection>,
    >,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let projection_policy_changed = !changed_lighting_policies.is_empty()
        || !changed_render_layers.is_empty()
        || !changed_effect_materials.is_empty();
    for (root, expanded) in &roots {
        if !expanded.is_added() && !projection_policy_changed {
            continue;
        }
        let lighting_policy = find_nearest_fixed_function_world_lighting_policy(root, &hierarchy);
        let lighting_override = lighting_policy.map(|policy| policy.enabled);
        for entity in
            std::iter::once(root).chain(children.iter_descendants_depth_first::<Children>(root))
        {
            let Ok((entity, current, projected, effect_source_layers)) = primitives.get(entity)
            else {
                continue;
            };
            let render_layers = effect_source_layers.map_or_else(
                || {
                    render_layers_for_prefab_entity_and_ancestor_chain(
                        entity,
                        &render_layer_hierarchy,
                    )
                },
                |source_layers| source_layers.0.clone(),
            );
            let world_light_context = render_layers
                .iter()
                .next()
                .filter(|context| *context < FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT)
                .unwrap_or_default();
            if lighting_override.is_none() && world_light_context == 0 {
                continue;
            }
            if projected.is_some_and(|projected| projected.0 == current.0) {
                continue;
            }
            let Some(material) = materials.get(&current.0).and_then(|material| {
                material.clone_with_fixed_function_world_lighting_context(
                    lighting_override,
                    lighting_policy
                        .is_some_and(|policy| policy.preserve_authored_material_lighting),
                    world_light_context,
                    &mut effect_uniform_buffers,
                )
            }) else {
                continue;
            };
            let handle = materials.add(material);
            commands.entity(entity).insert((
                MeshMaterial3d(handle.clone()),
                ProjectedPrefabFixedFunctionWorldLightingMaterial(handle),
            ));
        }
    }
}

pub(super) fn update_fixed_function_world_light_buffer(
    ambient_light: Res<GlobalAmbientLight>,
    camera_ambient_lights: Query<(&Camera, &AmbientLight, Option<&RenderLayers>), With<Camera3d>>,
    directional_lights: Query<(
        &DirectionalLight,
        &GlobalTransform,
        Option<&RenderLayers>,
        &ViewVisibility,
    )>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut previous_contexts: Local<Option<D3d9FixedFunctionWorldLightContexts>>,
) {
    let contexts = D3d9FixedFunctionWorldLightContexts {
        values: std::array::from_fn(|layer| {
            collect_fixed_function_world_light_registers_for_render_layers(
                &ambient_light,
                &camera_ambient_lights,
                &directional_lights,
                &RenderLayers::layer(layer),
            )
        }),
    };
    if previous_contexts.as_ref() == Some(&contexts) {
        return;
    }
    let Some(mut buffer) = buffers.get_mut(FIXED_FUNCTION_WORLD_LIGHT_BUFFER.id()) else {
        return;
    };
    buffer.set_data(contexts);
    *previous_contexts = Some(contexts);
}

fn collect_fixed_function_world_light_registers_for_render_layers(
    ambient_light: &GlobalAmbientLight,
    camera_ambient_lights: &Query<(&Camera, &AmbientLight, Option<&RenderLayers>), With<Camera3d>>,
    directional_lights: &Query<(
        &DirectionalLight,
        &GlobalTransform,
        Option<&RenderLayers>,
        &ViewVisibility,
    )>,
    render_layers: &RenderLayers,
) -> D3d9FixedFunctionWorldLightRegisters {
    let mut registers = D3d9FixedFunctionWorldLightRegisters::default();
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
    // final output to linear. Decode neither the textures nor these lights
    // ahead of that calculation; Bevy's PBR lighting remains linear elsewhere.
    registers.ambient_color_and_directional_light_count =
        Vec4::from_array(ambient_color.to_srgba().to_f32_array()) * ambient_brightness;
    let mut directional_light_count = 0;
    for (light, transform, light_layers, view_visibility) in directional_lights {
        if !view_visibility.get()
            || !light_layers.unwrap_or_default().intersects(render_layers)
            || directional_light_count == D3D9_MAX_ACTIVE_DIRECTIONAL_LIGHTS
        {
            continue;
        }
        registers.directional_light_directions[directional_light_count] =
            Vec3::from(transform.back()).extend(0.0);
        registers.directional_light_colors[directional_light_count] =
            Vec4::from_array(light.color.to_srgba().to_f32_array()) * light.illuminance;
        directional_light_count += 1;
    }
    registers.ambient_color_and_directional_light_count.w = directional_light_count as f32;
    registers
}

fn find_nearest_fixed_function_world_lighting_policy(
    mut entity: Entity,
    hierarchy: &Query<(
        Option<&PrefabFixedFunctionWorldLightingPolicy>,
        Option<&ChildOf>,
    )>,
) -> Option<PrefabFixedFunctionWorldLightingPolicy> {
    loop {
        let Ok((policy, parent)) = hierarchy.get(entity) else {
            return None;
        };
        if policy.is_some() {
            return policy.copied();
        }
        entity = parent?.parent();
    }
}
