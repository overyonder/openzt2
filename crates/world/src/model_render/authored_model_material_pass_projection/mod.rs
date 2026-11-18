use bevy::asset::AssetEvent;
use bevy::camera::visibility::RenderLayers;
use bevy::gltf::GltfMaterialName;
use bevy::light::NotShadowCaster;
use bevy::light::NotShadowReceiver;
use bevy::mesh::morph::MeshMorphWeights;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabClothingTextures;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPrimitiveMaterial;

pub(super) fn project_authored_model_material_passes(
    mut commands: Commands,
    roots: Query<
        (
            Entity,
            &PrefabMaterialOverrides,
            Option<&PrefabClothingTextures>,
        ),
        (
            With<ModelExpanded>,
            Without<AuthoredModelMaterialPassesProjected>,
        ),
    >,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    primitives: Query<(
        Entity,
        &GltfMaterialName,
        &Mesh3d,
        Option<&RenderLayers>,
        Has<NotShadowCaster>,
        Has<NotShadowReceiver>,
        Option<&SkinnedMesh>,
        Option<&MeshMorphWeights>,
    )>,
    materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut writable_effect_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
) {
    for (root, overrides, clothing) in &roots {
        let source_parent = parents.get(root).ok().map(ChildOf::parent).unwrap_or(root);
        let source_index = children
            .get(source_parent)
            .ok()
            .and_then(|children| children.iter().position(|child| child == root))
            .unwrap_or(0);
        if std::iter::once(root)
            .chain(children.iter_descendants_depth_first::<Children>(root))
            .all(|entity| !primitives.contains(entity))
        {
            continue;
        }
        if overrides.0.iter().any(|material_override| {
            material_override
                .handle
                .as_ref()
                .is_none_or(|handle| materials.get(handle).is_none())
        }) {
            continue;
        }
        let mut matched_primitive_count = 0_usize;
        let all_matched_effect_pass_materials_are_loaded = std::iter::once(root)
            .chain(children.iter_descendants_depth_first::<Children>(root))
            .filter_map(|entity| primitives.get(entity).ok())
            .filter_map(|(_, name, _, _, _, _, _, _)| {
                overrides
                    .0
                    .iter()
                    .find(|material_override| {
                        name.0.eq_ignore_ascii_case(
                            &material_override.material.to_lowercase_hexadecimal_string(),
                        )
                    })
                    .and_then(|material_override| material_override.handle.as_ref())
                    .map(|handle| {
                        matched_primitive_count += 1;
                        handle
                    })
            })
            .all(|handle| {
                materials.get(handle).is_some_and(|material| {
                    material
                        .evaluated_pass_material_assets_for_effect_quality(
                            graphics_settings.effects,
                        )
                        .iter()
                        .all(|pass| writable_effect_materials.get(pass).is_some())
                })
            });
        if !all_matched_effect_pass_materials_are_loaded
            || (!overrides.0.is_empty() && matched_primitive_count == 0)
        {
            continue;
        }
        for entity in
            std::iter::once(root).chain(children.iter_descendants_depth_first::<Children>(root))
        {
            let Ok((entity, name, mesh, layers, no_shadow_cast, no_shadow_receive, skin, morph)) =
                primitives.get(entity)
            else {
                continue;
            };
            let Some(handle) = overrides
                .0
                .iter()
                .find(|material_override| {
                    name.0.eq_ignore_ascii_case(
                        &material_override.material.to_lowercase_hexadecimal_string(),
                    )
                })
                .and_then(|material_override| material_override.handle.as_ref())
            else {
                continue;
            };
            let Some(material) = materials.get(handle) else {
                continue;
            };
            let mut target = commands.entity(entity);
            target.insert(PrefabPrimitiveMaterial(handle.clone()));
            let projected_passes = material
                .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
                .iter()
                .filter_map(|handle| {
                    let template = writable_effect_materials.get(handle)?;
                    let clothed =
                        material
                            .netimmerse_texture_binding
                            .as_ref()
                            .and_then(|binding| {
                                let texture = clothing?.0.iter().find(|(id, _)| {
                                    name.0
                                        .eq_ignore_ascii_case(&id.to_lowercase_hexadecimal_string())
                                })?;
                                Some(template.clone_with_netimmerse_decal(
                                    binding.decal_stage,
                                    binding.decal_uv_set,
                                    texture.1.clone(),
                                    &mut effect_uniform_buffers,
                                ))
                            });
                    // The shader computes every camera- and model-dependent
                    // input, so each draw shares its pass material.
                    Some(clothed.map_or_else(
                        || handle.clone(),
                        |material| writable_effect_materials.add(material),
                    ))
                })
                .collect::<Vec<_>>();
            let Some(first_pass) = projected_passes.first().cloned() else {
                target.insert(Visibility::Hidden);
                continue;
            };
            target
                .insert(Visibility::Inherited)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert((
                    MeshMaterial3d::<EffectPassMaterial>(first_pass),
                    AuthoredEffectTechniquePassSubmissionOrder::new(entity, 0)
                        .expect("the first authored effect pass index fits u16")
                        .with_source_hierarchy_order(source_parent, source_index),
                ));
            for (pass_index, pass) in projected_passes.iter().enumerate().skip(1) {
                let mut additional_pass = commands.spawn((
                    mesh.clone(),
                    MeshMaterial3d::<EffectPassMaterial>(pass.clone()),
                    Transform::default(),
                    Visibility::Inherited,
                    ChildOf(entity),
                    AdditionalAuthoredModelMaterialPass,
                    AuthoredEffectTechniquePassSubmissionOrder::new(entity, pass_index)
                        .expect("the authored effect pass index fits u16")
                        .with_source_hierarchy_order(source_parent, source_index),
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
            }
        }
        commands
            .entity(root)
            .insert(AuthoredModelMaterialPassesProjected);
    }
}

pub(super) fn invalidate_authored_model_material_pass_projection(
    mut events: MessageReader<AssetEvent<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut commands: Commands,
    roots: Query<
        (
            Entity,
            &PrefabMaterialOverrides,
            Option<Ref<PrefabClothingTextures>>,
        ),
        With<AuthoredModelMaterialPassesProjected>,
    >,
    children: Query<&Children>,
    additional_passes: Query<(), With<AdditionalAuthoredModelMaterialPass>>,
    changed_clothing: Query<(), Changed<PrefabClothingTextures>>,
) {
    let changed_materials = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(*id),
            AssetEvent::Unused { .. } => None,
        })
        .collect::<HashSet<_>>();
    let all_materials_changed = graphics_settings.is_changed();
    if changed_materials.is_empty() && !all_materials_changed && changed_clothing.is_empty() {
        return;
    }
    for (root, overrides, clothing) in &roots {
        let root_material_changed = clothing.is_some_and(|clothing| clothing.is_changed())
            || all_materials_changed
            || overrides.0.iter().any(|material_override| {
                material_override
                    .handle
                    .as_ref()
                    .is_some_and(|handle| changed_materials.contains(&handle.id()))
            });
        if !root_material_changed {
            continue;
        }
        commands
            .entity(root)
            .remove::<AuthoredModelMaterialPassesProjected>();
        for descendant in children.iter_descendants_depth_first(root) {
            if additional_passes.contains(descendant) {
                commands.entity(descendant).despawn();
            }
        }
    }
}

#[derive(Component)]
pub(crate) struct AuthoredModelMaterialPassesProjected;

#[derive(Component)]
pub(crate) struct AdditionalAuthoredModelMaterialPass;
