use super::scoped_prefab_lighting::ScopedPrefabLightInputs;
use crate::animation_playback::authored_decal_texture_cycles::AnimationDecalFrames;
use crate::world_spawn::prefab_world_instance_spawning::morph_fade_projection::{
    projected_prefab_opacity, NamedPrefabOpacity, PrefabMorphOpacity,
};
use bevy::app::{HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::asset::{asset_changed::AssetChanged, AsAssetId};
use bevy::asset::{AssetEvent, AssetId};
use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::system::SystemParam;
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
use super::environment_layer_rendering::FIRST_ENVIRONMENT_RENDER_LAYER;
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

impl AsAssetId for AuthoredModelEffectMaterial {
    type Asset = EffectPassMaterial;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.template.id()
    }
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

#[derive(Clone, PartialEq, Eq, Hash)]
struct ModelEffectMaterialVariant {
    authored: AssetId<EffectPassMaterial>,
    replacement: Option<AssetId<Image>>,
    clothing_decal: Option<(usize, u32, AssetId<Image>)>,
    light_policy: Option<(bool, bool)>,
    light_context: usize,
    tint: Option<[u32; 4]>,
    animation_geometry: Option<Entity>,
    ordered_environment: bool,
    fading_draw: Option<Entity>,
    scoped_lighting_draw: Option<Entity>,
    animated_decals: Box<[(Option<usize>, Option<u32>, AssetId<Image>)]>,
}

pub(super) fn register_configured_model_effect_materials(app: &mut App) {
    register_model_material_input::<ModelExpanded>(app);
    register_model_material_input::<PrefabTextureReplacements>(app);
    register_model_material_input::<PrefabFixedFunctionWorldLightingPolicy>(app);
    register_model_material_input::<AnimationDecalFrames>(app);

    app.configure_sets(
        PostUpdate,
        (
            PropagateSet::<ModelMaterialInputOwner<ModelExpanded>>::default()
                .before(project_authored_model_material_passes),
            PropagateSet::<PrefabMorphOpacity>::default()
                .after(project_authored_model_material_passes),
            PropagateSet::<NamedPrefabOpacity>::default()
                .after(project_authored_model_material_passes),
        ),
    );
    app.add_systems(
        PostUpdate,
        project_configured_model_effect_materials
            .after(PropagateSet::<PrefabMorphOpacity>::default())
            .after(PropagateSet::<NamedPrefabOpacity>::default())
            .after(project_mirrored_and_tinted_model_primitive_meshes)
            .after(PropagateSet::<PrefabCameraRenderLayers>::default())
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

#[derive(SystemParam)]
pub(super) struct AnimatedMaterialInputs<'w, 's> {
    decals: Query<'w, 's, Ref<'static, AnimationDecalFrames>>,
    removed_decals: RemovedComponents<'w, 's, ModelMaterialInputOwner<AnimationDecalFrames>>,
    removed_morph: RemovedComponents<'w, 's, PrefabMorphOpacity>,
    removed_named: RemovedComponents<'w, 's, NamedPrefabOpacity>,
}

#[derive(SystemParam)]
pub(super) struct ConfiguredMaterialChanges<'w, 's> {
    draws: Query<
        'w,
        's,
        Entity,
        (
            With<AuthoredModelEffectMaterial>,
            Or<(
                Changed<AuthoredModelEffectMaterial>,
                AssetChanged<AuthoredModelEffectMaterial>,
                Changed<Mesh3d>,
                AssetChanged<Mesh3d>,
                Changed<PrefabCameraRenderLayers>,
                Changed<ModelMaterialInputOwner<ModelExpanded>>,
                Changed<ModelMaterialInputOwner<PrefabTextureReplacements>>,
                Changed<ModelMaterialInputOwner<AnimationDecalFrames>>,
                Changed<ModelMaterialInputOwner<PrefabFixedFunctionWorldLightingPolicy>>,
                Changed<InheritedPrimitiveTint>,
                Changed<PrefabMorphOpacity>,
                Changed<NamedPrefabOpacity>,
            )>,
        ),
    >,
    owners: Query<
        'w,
        's,
        Entity,
        Or<(
            Changed<PrefabModel>,
            Changed<PrefabTextureReplacements>,
            Changed<PrefabFixedFunctionWorldLightingPolicy>,
            Changed<AnimationDecalFrames>,
            Changed<ScopedPrefabLightInputs>,
        )>,
    >,
    scoped_lights: Query<'w, 's, &'static ScopedPrefabLightInputs>,
    parents: Query<'w, 's, &'static ChildOf>,
    removed_lights: RemovedComponents<'w, 's, ScopedPrefabLightInputs>,
    children: Query<'w, 's, &'static Children>,
    texture_overrides: Query<
        'w,
        's,
        Entity,
        (
            With<AuthoredModelEffectMaterial>,
            With<ModelMaterialInputOwner<PrefabTextureReplacements>>,
        ),
    >,
    asset_dependencies: Query<'w, 's, (Entity, &'static AuthoredModelEffectMaterial)>,
}

pub(super) fn project_configured_model_effect_materials(
    mut dirty_draws: ConfiguredMaterialChanges,
    mut draws: Query<(
        Entity,
        Ref<AuthoredModelEffectMaterial>,
        Ref<Mesh3d>,
        Option<Ref<PrefabCameraRenderLayers>>,
        Option<Ref<ModelMaterialInputOwner<ModelExpanded>>>,
        Option<Ref<ModelMaterialInputOwner<PrefabTextureReplacements>>>,
        Option<Ref<InheritedPrimitiveTint>>,
        Option<&PrefabMorphOpacity>,
        Option<&NamedPrefabOpacity>,
        Option<Ref<ModelMaterialInputOwner<AnimationDecalFrames>>>,
        Option<&bevy::gltf::GltfMaterialName>,
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
    mut animation: AnimatedMaterialInputs,
    mut variants: Local<HashMap<ModelEffectMaterialVariant, AssetId<EffectPassMaterial>>>,
) {
    let mut removed_assets = HashSet::new();
    let changed_assets = material_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(id),
            AssetEvent::Removed { id } => {
                removed_assets.insert(id);
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
        .chain(animation.removed_morph.read())
        .chain(animation.removed_named.read())
        .chain(animation.removed_decals.read())
        .collect::<EntityHashSet>();
    let texture_replacement_enabled = graphics
        .as_ref()
        .is_none_or(|graphics| graphics.texture_replacement);
    let changed_settings = graphics
        .as_ref()
        .is_some_and(|graphics| graphics.is_changed());

    let mut affected = removed_inputs;
    affected.extend(dirty_draws.draws.iter());
    for owner in dirty_draws
        .owners
        .iter()
        .chain(dirty_draws.removed_lights.read())
    {
        affected.extend(
            std::iter::once(owner).chain(dirty_draws.children.iter_descendants::<Children>(owner)),
        );
    }
    if changed_settings {
        affected.extend(dirty_draws.texture_overrides.iter());
    }
    if !removed_assets.is_empty() {
        affected.extend(
            dirty_draws
                .asset_dependencies
                .iter()
                .filter_map(|(entity, material)| {
                    removed_assets
                        .contains(&material.template.id())
                        .then_some(entity)
                }),
        );
    }
    let mut draws = draws.iter_many_mut(affected).matched();
    while let Some((
        entity,
        authored,
        mesh,
        layers,
        model_owner,
        replacement_owner,
        tint,
        morph,
        named,
        decal_owner,
        material_name,
        mut binding,
    )) = draws.fetch_next()
    {
        let opacity = projected_prefab_opacity(morph, named);
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
        let decals = decal_owner
            .as_ref()
            .and_then(|owner| animation.decals.get(owner.0).ok());
        let scoped_lights = std::iter::once(authored.geometry)
            .chain(
                dirty_draws
                    .parents
                    .iter_ancestors::<ChildOf>(authored.geometry),
            )
            .filter_map(|entity| dirty_draws.scoped_lights.get(entity).ok())
            .flat_map(|lights| lights.0.iter().copied())
            .collect::<Vec<_>>();
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
            binding.set_if_neq(MeshMaterial3d(authored.template.clone()));
            continue;
        };
        let animated_decals = decals
            .as_ref()
            .map_or(&[][..], |decals| decals.0.as_slice())
            .iter()
            .filter(|decal| {
                model
                    .as_ref()
                    .and_then(|(model, _)| model.material_name.as_deref())
                    .is_some_and(|name| name.eq_ignore_ascii_case(&decal.material))
                    || material_name.is_some_and(|name| {
                        name.0.eq_ignore_ascii_case(&decal.material)
                            || name.0.eq_ignore_ascii_case(
                                &openzt2_game_data::AssetId::from_key(&decal.material)
                                    .to_lowercase_hexadecimal_string(),
                            )
                    })
            })
            .collect::<Vec<_>>();
        let animation_geometry = source
            .has_texture_coordinate_animation()
            .then_some(authored.geometry);
        let ordered_environment = source.uses_binned_opaque_phase()
            && layers.as_ref().is_some_and(|layers| {
                layers
                    .0
                    .iter()
                    .any(|layer| layer >= FIRST_ENVIRONMENT_RENDER_LAYER)
            });
        if texture.is_none()
            && light_policy.is_none()
            && light_context == 0
            && tint.is_none()
            && authored.clothing_decal.is_none()
            && animation_geometry.is_none()
            && !ordered_environment
            && opacity.is_none()
            && animated_decals.is_empty()
            && scoped_lights.is_empty()
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
            fading_draw: opacity.map(|_| entity),
            scoped_lighting_draw: (!scoped_lights.is_empty()).then_some(entity),
            animated_decals: animated_decals
                .iter()
                .map(|decal| (decal.layer, decal.texture_coordinates, decal.image.id()))
                .collect(),
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
            for decal in animated_decals {
                if let Some(stage) = decal.layer {
                    let texture_coordinates = decal
                        .texture_coordinates
                        .unwrap_or_else(|| derived.netimmerse_texture_coordinate_set(stage));
                    derived = derived.clone_with_netimmerse_decal(
                        stage,
                        texture_coordinates,
                        decal.image.clone(),
                        &mut buffers,
                    );
                } else {
                    derived.replace_texture_asset_at_d3d9_stage(0, decal.image.clone());
                }
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
            if opacity.is_some() {
                derived.configure_for_morph_fade();
            }
            if !scoped_lights.is_empty() {
                derived.bind_scoped_prefab_lighting(&scoped_lights, &mut buffers);
            }
            let handle = materials.add(derived);
            variants.insert(key, handle.id());
            handle
        };
        if !scoped_lights.is_empty() {
            if let Some(material) = materials.get(&handle) {
                material.update_scoped_prefab_lighting(&scoped_lights, &mut buffers);
            }
        }
        if let Some(opacity) = opacity {
            if materials
                .get(&handle)
                .is_some_and(|material| material.morph_opacity() != opacity)
            {
                if let Some(mut material) = materials.get_mut(&handle) {
                    material.set_morph_opacity(opacity);
                }
            }
        }
        binding.set_if_neq(MeshMaterial3d(handle));
    }
}
