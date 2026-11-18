use super::configured_model_effect_materials::{
    AuthoredModelEffectMaterial, ModelMaterialInputOwner,
};
use super::model_scene_visibility_and_shadow_projection::ProjectedModelDrawShadowPolicy;
use bevy::asset::AssetEvent;
use bevy::camera::visibility::RenderLayers;
use bevy::gltf::GltfMaterialName;
use bevy::light::NotShadowCaster;
use bevy::light::NotShadowReceiver;
use bevy::mesh::morph::MeshMorphWeights;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::material::runtime::texture_coordinate_animation::GeometryTextureCoordinateAnimationClock;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabClothingTextures;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPrimitiveMaterial;

pub(super) fn initialize_model_geometry_texture_coordinate_clocks(
    mut commands: Commands,
    time: Res<Time>,
    geometries: Query<
        Entity,
        (
            With<Mesh3d>,
            With<GltfMaterialName>,
            Or<(Added<Mesh3d>, Added<GltfMaterialName>)>,
            Without<GeometryTextureCoordinateAnimationClock>,
        ),
    >,
) {
    let started = time.elapsed_secs();
    for entity in &geometries {
        commands
            .entity(entity)
            .insert(GeometryTextureCoordinateAnimationClock::new(started));
    }
}

#[derive(Component)]
pub(super) struct AdditionalAuthoredModelMaterialPass;

#[derive(Component)]
pub(super) struct AuthoredModelStandardMaterial {
    pub(super) source: Handle<StandardMaterial>,
    pub(super) rendered_material: bevy::asset::AssetId<StandardMaterial>,
}

pub(super) fn project_authored_model_material_passes(
    mut commands: Commands,
    primitives: Query<(
        Entity,
        Ref<GltfMaterialName>,
        &Mesh3d,
        Option<Ref<ModelMaterialInputOwner<ModelExpanded>>>,
        Option<&RenderLayers>,
        Has<NotShadowCaster>,
        Has<NotShadowReceiver>,
        Option<&SkinnedMesh>,
        Option<&MeshMorphWeights>,
        Option<&ProjectedModelDrawShadowPolicy>,
        Option<&MeshMaterial3d<StandardMaterial>>,
        Option<&AuthoredModelStandardMaterial>,
        Option<&PrefabPrimitiveMaterial>,
        Has<AuthoredModelEffectMaterial>,
    )>,
    roots: Query<(
        Ref<PrefabMaterialOverrides>,
        Option<Ref<PrefabClothingTextures>>,
    )>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    additional_passes: Query<(), With<AdditionalAuthoredModelMaterialPass>>,
    mut material_events: MessageReader<AssetEvent<MaterialAsset>>,
    mut pass_events: MessageReader<AssetEvent<EffectPassMaterial>>,
    mut removed_clothing: RemovedComponents<PrefabClothingTextures>,
    mut removed_overrides: RemovedComponents<PrefabMaterialOverrides>,
    mut removed_owners: RemovedComponents<ModelMaterialInputOwner<ModelExpanded>>,
    material_assets: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    draw_materials: Res<Assets<EffectPassMaterial>>,
    mut visibility: Query<&mut Visibility, With<GltfMaterialName>>,
    mut authored_effects: Query<&mut AuthoredModelEffectMaterial>,
) {
    let changed_materials = material_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            AssetEvent::Unused { .. } => None,
        })
        .collect::<HashSet<_>>();
    let removed_sources = removed_clothing
        .read()
        .chain(removed_overrides.read())
        .chain(removed_owners.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let changed_passes = pass_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect::<HashSet<_>>();

    for (
        entity,
        name,
        mesh,
        owner,
        layers,
        no_shadow_cast,
        no_shadow_receive,
        skin,
        morph,
        shadow_projection,
        standard,
        authored_standard,
        previous_material,
        has_effect_binding,
    ) in &primitives
    {
        let root = owner.as_ref().and_then(|owner| roots.get(owner.0).ok());
        let source_changed = name.is_changed()
            || removed_sources.contains(&entity)
            || owner
                .as_ref()
                .is_some_and(|owner| owner.is_changed() || removed_sources.contains(&owner.0))
            || root.as_ref().is_some_and(|(overrides, clothing)| {
                overrides.is_changed()
                    || clothing
                        .as_ref()
                        .is_some_and(|clothing| clothing.is_changed())
            });
        if !source_changed
            && !graphics_settings.is_changed()
            && changed_materials.is_empty()
            && changed_passes.is_empty()
        {
            continue;
        }
        let override_material = root.as_ref().and_then(|(overrides, _)| {
            overrides.0.iter().find(|material_override| {
                name.0.eq_ignore_ascii_case(
                    &material_override.material.to_lowercase_hexadecimal_string(),
                )
            })
        });
        let Some(override_material) = override_material else {
            if previous_material.is_some() {
                despawn_authored_material_pass_children(
                    &mut commands,
                    entity,
                    &children,
                    &additional_passes,
                );
                let mut target = commands.entity(entity);
                target.remove::<(
                    AuthoredModelEffectMaterial,
                    MeshMaterial3d<EffectPassMaterial>,
                    PrefabPrimitiveMaterial,
                    AuthoredEffectTechniquePassSubmissionOrder,
                    AuthoredModelStandardMaterial,
                )>();
                if let Some(standard) = authored_standard {
                    target.insert(MeshMaterial3d(standard.source.clone()));
                }
                if let Ok(mut visibility) = visibility.get_mut(entity) {
                    visibility.set_if_neq(Visibility::Inherited);
                }
            }
            continue;
        };
        let Some(handle) = override_material.handle.as_ref() else {
            continue;
        };
        let Some(material) = material_assets.get(handle) else {
            continue;
        };
        let Some(owner) = owner else { continue };
        let clothing = root.as_ref().and_then(|(_, clothing)| clothing.as_ref());
        let passes =
            material.evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects);
        if !source_changed
            && !graphics_settings.is_changed()
            && !changed_materials.contains(&handle.id())
            && !passes
                .iter()
                .any(|pass| changed_passes.contains(&pass.id()))
        {
            continue;
        }
        if passes
            .iter()
            .any(|pass| !draw_materials.contains(pass.id()))
        {
            continue;
        }

        despawn_authored_material_pass_children(
            &mut commands,
            entity,
            &children,
            &additional_passes,
        );
        commands
            .entity(entity)
            .insert(PrefabPrimitiveMaterial(handle.clone()));
        if passes.is_empty() {
            if let Ok(mut visibility) = visibility.get_mut(entity) {
                visibility.set_if_neq(Visibility::Hidden);
            }
            continue;
        }
        if let Ok(mut visibility) = visibility.get_mut(entity) {
            visibility.set_if_neq(Visibility::Inherited);
        }

        let owner_parent = parents
            .get(owner.0)
            .ok()
            .map(ChildOf::parent)
            .unwrap_or(owner.0);
        let owner_index = children
            .get(owner_parent)
            .ok()
            .and_then(|children| children.iter().position(|child| child == owner.0))
            .unwrap_or(0);
        let decal = material
            .netimmerse_texture_binding
            .as_ref()
            .and_then(|binding| {
                let texture = clothing.as_ref()?.0.iter().find(|(id, _)| {
                    name.0
                        .eq_ignore_ascii_case(&id.to_lowercase_hexadecimal_string())
                })?;
                Some((binding.decal_stage, binding.decal_uv_set, texture.1.clone()))
            });
        for (pass_index, pass) in passes.iter().enumerate() {
            let authored = AuthoredModelEffectMaterial {
                template: pass.clone(),
                clothing_decal: decal.clone(),
                geometry: entity,
            };
            let ordering = AuthoredEffectTechniquePassSubmissionOrder::new(entity, pass_index)
                .expect("the authored effect pass index fits u16")
                .with_source_hierarchy_order(owner_parent, owner_index);
            if pass_index == 0 {
                let mut target = commands.entity(entity);
                if !has_effect_binding {
                    if let Some(standard) = standard.filter(|_| authored_standard.is_none()) {
                        target.insert(AuthoredModelStandardMaterial {
                            source: standard.0.clone(),
                            rendered_material: standard.id(),
                        });
                    }
                    target.remove::<MeshMaterial3d<StandardMaterial>>();
                    target.insert(MeshMaterial3d(pass.clone()));
                }
                if let Ok(mut current) = authored_effects.get_mut(entity) {
                    current.set_if_neq(authored);
                } else {
                    target.insert(authored);
                }
                target.insert(ordering);
                continue;
            }
            let mut additional_pass = commands.spawn((
                mesh.clone(),
                authored,
                MeshMaterial3d(pass.clone()),
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(entity),
                AdditionalAuthoredModelMaterialPass,
                ordering,
            ));
            if let Some(layers) = layers {
                additional_pass.insert(layers.clone());
            }
            if no_shadow_cast {
                additional_pass.insert(NotShadowCaster);
            }
            if no_shadow_receive {
                additional_pass.insert(NotShadowReceiver);
            }
            if let Some(skin) = skin {
                additional_pass.insert(skin.clone());
            }
            if let Some(morph) = morph {
                additional_pass.insert(morph.clone());
            }
            if let Some(shadow_projection) = shadow_projection {
                additional_pass.insert(*shadow_projection);
            }
        }
    }
}

fn despawn_authored_material_pass_children(
    commands: &mut Commands,
    primitive: Entity,
    children: &Query<&Children>,
    additional_passes: &Query<(), With<AdditionalAuthoredModelMaterialPass>>,
) {
    for child in children
        .get(primitive)
        .into_iter()
        .flat_map(|children| children.iter())
    {
        if additional_passes.contains(child) {
            commands.entity(child).try_despawn();
        }
    }
}
