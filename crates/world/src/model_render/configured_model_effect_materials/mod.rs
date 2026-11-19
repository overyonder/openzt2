use bevy::app::{HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::asset::{AssetEvent, AssetId};
use bevy::ecs::entity::EntityHashSet;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::{
    EffectPassMaterial, FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT,
};
use openzt2_world_state::model_render::{ModelExpanded, ModelRenderPresentationSet};
use openzt2_world_state::world_spawn::{
    prefab_fixed_function_world_lighting_policy::PrefabFixedFunctionWorldLightingPolicy,
    prefab_presentation_types::PrefabModel, prefab_texture_replacements::PrefabTextureReplacements,
};
use std::marker::PhantomData;

use super::authored_model_material_pass_projection::project_authored_model_material_passes;
use super::environment_layer_rendering::EnvironmentPrimitiveLayer;
use super::mirrored_model_primitive_winding::{
    project_mirrored_and_tinted_model_primitive_meshes, InheritedPrimitiveTint,
};
use super::prefab_render_layer_camera_matching::PrefabCameraRenderLayers;

#[derive(Component, PartialEq, Eq)]
pub(super) struct AuthoredModelEffectMaterial {
    pub(super) template: Handle<EffectPassMaterial>,
    pub(super) clothing_decal: Option<(usize, u32, Handle<Image>)>,
    pub(super) geometry: Entity,
}

#[derive(Component)]
pub(super) struct ModelMaterialInputOwner<T: Component>(pub(super) Entity, PhantomData<fn() -> T>);

impl<T: Component> Clone for ModelMaterialInputOwner<T> {
    fn clone(&self) -> Self {
        Self(self.0, PhantomData)
    }
}

impl<T: Component> PartialEq for ModelMaterialInputOwner<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ModelEffectMaterialVariant {
    authored: AssetId<EffectPassMaterial>,
    replacement: Option<AssetId<Image>>,
    clothing_decal: Option<(usize, u32, AssetId<Image>)>,
    light_policy: Option<(bool, bool)>,
    light_context: usize,
    tint: Option<[u32; 4]>,
    animation_geometry: Option<Entity>,
    ordered_environment: bool,
}

pub(super) fn register_configured_model_effect_materials(app: &mut App) {
    register_model_material_input::<ModelExpanded>(app);
    register_model_material_input::<PrefabTextureReplacements>(app);
    register_model_material_input::<PrefabFixedFunctionWorldLightingPolicy>(app);

    app.configure_sets(
        PostUpdate,
        PropagateSet::<ModelMaterialInputOwner<ModelExpanded>>::default()
            .before(project_authored_model_material_passes),
    );
    app.add_systems(
        PostUpdate,
        project_configured_model_effect_materials
            .after(project_authored_model_material_passes)
            .after(project_mirrored_and_tinted_model_primitive_meshes)
            .after(PropagateSet::<PrefabCameraRenderLayers>::default())
            .after(PropagateSet::<EnvironmentPrimitiveLayer>::default())
            .in_set(ModelRenderPresentationSet::SourceMaterials)
            .before(bevy::pbr::check_entities_needing_specialization::<EffectPassMaterial>),
    );
}

fn register_model_material_input<T: Component>(app: &mut App) {
    app.add_plugins(HierarchyPropagatePlugin::<ModelMaterialInputOwner<T>>::new(
        PostUpdate,
    ))
    .add_systems(
        PostUpdate,
        publish_model_material_input_owners::<T>
            .before(PropagateSet::<ModelMaterialInputOwner<T>>::default()),
    )
    .configure_sets(
        PostUpdate,
        PropagateSet::<ModelMaterialInputOwner<T>>::default()
            .before(project_configured_model_effect_materials),
    );
}

fn publish_model_material_input_owners<T: Component>(
    mut commands: Commands,
    added: Query<Entity, Added<T>>,
    present: Query<(), With<T>>,
    mut removed: RemovedComponents<T>,
) {
    for entity in &added {
        commands
            .entity(entity)
            .insert(Propagate(ModelMaterialInputOwner::<T>(entity, PhantomData)));
    }
    for entity in removed.read() {
        if !present.contains(entity) {
            if let Ok(mut target) = commands.get_entity(entity) {
                target.remove::<Propagate<ModelMaterialInputOwner<T>>>();
            }
        }
    }
}

fn project_configured_model_effect_materials(
    mut draws: Query<(
        Entity,
        Ref<AuthoredModelEffectMaterial>,
        Ref<Mesh3d>,
        Option<Ref<PrefabCameraRenderLayers>>,
        Option<Ref<EnvironmentPrimitiveLayer>>,
        Option<Ref<ModelMaterialInputOwner<ModelExpanded>>>,
        Option<Ref<ModelMaterialInputOwner<PrefabTextureReplacements>>>,
        Option<Ref<InheritedPrimitiveTint>>,
        &mut MeshMaterial3d<EffectPassMaterial>,
    )>,
    models: Query<(
        Ref<PrefabModel>,
        Option<Ref<ModelMaterialInputOwner<PrefabFixedFunctionWorldLightingPolicy>>>,
    )>,
    replacements: Query<Ref<PrefabTextureReplacements>>,
    lighting_policies: Query<Ref<PrefabFixedFunctionWorldLightingPolicy>>,
    meshes: Res<Assets<Mesh>>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    graphics: Option<Res<GraphicsSettings>>,
    mut material_events: MessageReader<AssetEvent<EffectPassMaterial>>,
    mut removed_model_owners: RemovedComponents<ModelMaterialInputOwner<ModelExpanded>>,
    mut removed_replacement_owners: RemovedComponents<
        ModelMaterialInputOwner<PrefabTextureReplacements>,
    >,
    mut removed_tint_owners: RemovedComponents<InheritedPrimitiveTint>,
    mut removed_light_owners: RemovedComponents<
        ModelMaterialInputOwner<PrefabFixedFunctionWorldLightingPolicy>,
    >,
    mut removed_layers: RemovedComponents<PrefabCameraRenderLayers>,
    mut removed_environment: RemovedComponents<EnvironmentPrimitiveLayer>,
    mut variants: Local<HashMap<ModelEffectMaterialVariant, AssetId<EffectPassMaterial>>>,
) {
    let changed_assets = material_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect::<HashSet<_>>();
    if !changed_assets.is_empty() {
        variants.retain(|key, output| {
            !changed_assets.contains(&key.authored) && materials.contains(*output)
        });
    }
    let removed_inputs = removed_model_owners
        .read()
        .chain(removed_replacement_owners.read())
        .chain(removed_tint_owners.read())
        .chain(removed_light_owners.read())
        .chain(removed_layers.read())
        .chain(removed_environment.read())
        .collect::<EntityHashSet>();
    let texture_replacement_enabled = graphics
        .as_ref()
        .is_none_or(|graphics| graphics.texture_replacement);
    let changed_settings = graphics
        .as_ref()
        .is_some_and(|graphics| graphics.is_changed());

    for (
        entity,
        authored,
        mesh,
        layers,
        environment,
        model_owner,
        replacement_owner,
        tint,
        mut binding,
    ) in &mut draws
    {
        let model = model_owner
            .as_ref()
            .and_then(|owner| models.get(owner.0).ok());
        let replacement = replacement_owner
            .as_ref()
            .and_then(|owner| replacements.get(owner.0).ok());
        let light_policy = model
            .as_ref()
            .and_then(|(_, owner)| owner.as_ref())
            .and_then(|owner| lighting_policies.get(owner.0).ok());
        let changed = authored.is_changed()
            || mesh.is_changed()
            || removed_inputs.contains(&entity)
            || changed_assets.contains(&authored.template.id())
            || layers.as_ref().is_some_and(|layers| layers.is_changed())
            || environment.as_ref().is_some_and(|layer| layer.is_changed())
            || model_owner
                .as_ref()
                .is_some_and(|owner| owner.is_changed() || removed_inputs.contains(&owner.0))
            || replacement_owner
                .as_ref()
                .is_some_and(|owner| owner.is_changed())
            || model.as_ref().is_some_and(|(model, policy_owner)| {
                model.is_changed()
                    || policy_owner
                        .as_ref()
                        .is_some_and(|owner| owner.is_changed())
            })
            || replacement
                .as_ref()
                .is_some_and(|replacement| replacement.is_changed())
            || light_policy
                .as_ref()
                .is_some_and(|policy| policy.is_changed())
            || tint.as_ref().is_some_and(|tint| tint.is_changed())
            || (replacement.is_some() && changed_settings);
        if !changed {
            continue;
        }

        let texture = if texture_replacement_enabled {
            model
                .as_ref()
                .and_then(|(model, _)| model.material_name.as_deref())
                .and_then(|name| {
                    replacement
                        .as_ref()?
                        .0
                        .iter()
                        .find(|(material, _)| material.as_ref() == name)
                })
                .map(|(_, texture)| texture)
        } else {
            None
        };
        let light_context = if model.is_some() {
            layers
                .as_ref()
                .map(|layers| &layers.0)
                .unwrap_or_default()
                .iter()
                .next()
                .filter(|&layer| layer < FIXED_FUNCTION_WORLD_LIGHT_CONTEXT_COUNT)
                .unwrap_or_default()
        } else {
            0
        };
        let tint = tint
            .filter(|tint| tint.0 != Color::WHITE)
            .filter(|_| {
                meshes
                    .get(&mesh.0)
                    .is_some_and(|mesh| mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_none())
            })
            .map(|tint| tint.0.to_srgba().to_vec4());
        let Some(source) = materials.get(&authored.template) else {
            continue;
        };
        let animation_geometry = source
            .has_texture_coordinate_animation()
            .then_some(authored.geometry);
        let ordered_environment = environment.is_some() && source.uses_binned_opaque_phase();
        if texture.is_none()
            && light_policy.is_none()
            && light_context == 0
            && tint.is_none()
            && authored.clothing_decal.is_none()
            && animation_geometry.is_none()
            && !ordered_environment
        {
            binding.set_if_neq(MeshMaterial3d(authored.template.clone()));
            continue;
        }
        let key = ModelEffectMaterialVariant {
            authored: authored.template.id(),
            replacement: texture.map(Handle::id),
            clothing_decal: authored
                .clothing_decal
                .as_ref()
                .map(|(stage, uv, texture)| (*stage, *uv, texture.id())),
            light_policy: light_policy
                .as_ref()
                .map(|policy| (policy.enabled, policy.preserve_authored_material_lighting)),
            light_context,
            tint: tint.map(|color| color.to_array().map(f32::to_bits)),
            animation_geometry,
            ordered_environment,
        };
        let projected = variants
            .get(&key)
            .and_then(|&id| materials.get_strong_handle(id));
        let handle = if let Some(projected) = projected {
            projected
        } else {
            let Some(source) = materials.get(&authored.template) else {
                continue;
            };
            let mut derived = authored.clothing_decal.as_ref().map_or_else(
                || source.clone(),
                |(stage, uv, texture)| {
                    source.clone_with_netimmerse_decal(*stage, *uv, texture.clone(), &mut buffers)
                },
            );
            if light_policy.is_some() || light_context != 0 {
                if let Some(lit) = derived.clone_with_fixed_function_world_lighting_context(
                    light_policy.as_ref().map(|policy| policy.enabled),
                    light_policy
                        .as_ref()
                        .is_some_and(|policy| policy.preserve_authored_material_lighting),
                    light_context,
                    &mut buffers,
                ) {
                    derived = lit;
                }
            }
            if let Some(texture) = texture {
                derived.replace_texture_asset_at_d3d9_stage(0, texture.clone());
            }
            if let Some(tinted) = tint.and_then(|tint| derived.clone_tinted(tint, &mut buffers)) {
                derived = tinted;
            }
            if ordered_environment {
                derived.retain_authored_draw_order();
            }
            if let Some(geometry) = animation_geometry {
                derived.bind_texture_coordinate_animation_geometry(geometry, &mut buffers);
            }
            let handle = materials.add(derived);
            variants.insert(key, handle.id());
            handle
        };
        binding.set_if_neq(MeshMaterial3d(handle));
    }
}
