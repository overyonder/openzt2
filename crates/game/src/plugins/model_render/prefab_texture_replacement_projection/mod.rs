//! Binds an entity's drawn texture replacements onto its projected effect passes.

use bevy::prelude::*;

use crate::{
    assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial,
    plugins::world_spawn::{
        prefab_presentation_types::PrefabModel,
        prefab_texture_replacements::PrefabTextureReplacements,
    },
};

/// The replaced material this primitive currently draws with.
#[derive(Component)]
pub(super) struct ProjectedPrefabTextureReplacementMaterial(Handle<EffectPassMaterial>);

// ponytail: walks every owner's tree each frame; fine for staff counts, gate on
// change detection if owners grow to guests.
pub(super) fn project_prefab_texture_replacements_onto_effect_pass_materials(
    mut commands: Commands,
    owners: Query<(Entity, &PrefabTextureReplacements)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    models: Query<&PrefabModel>,
    primitives: Query<(
        &MeshMaterial3d<EffectPassMaterial>,
        Option<&ProjectedPrefabTextureReplacementMaterial>,
    )>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
) {
    for (owner, replacements) in &owners {
        for primitive in children.iter_descendants_depth_first::<Children>(owner) {
            let Ok((current, projected)) = primitives.get(primitive) else {
                continue;
            };
            if projected.is_some_and(|projected| projected.0 == current.0) {
                continue;
            }
            // A primitive belongs to its nearest renderable only: models
            // attached beneath another model's joints (a head on a body)
            // keep their own materials.
            let Some(texture) = std::iter::once(primitive)
                .chain(parents.iter_ancestors::<ChildOf>(primitive))
                .find_map(|entity| models.get(entity).ok())
                .and_then(|model| model.material_name.as_deref())
                .and_then(|name| {
                    replacements
                        .0
                        .iter()
                        .find(|(material, _)| material.as_ref() == name)
                })
                .map(|(_, texture)| texture)
            else {
                continue;
            };
            let Some(mut material) = materials.get(&current.0).cloned() else {
                continue;
            };
            // Native NIF materials bind their base map at stage 0.
            material.replace_texture_asset_at_d3d9_stage(0, texture.clone());
            let handle = materials.add(material);
            commands.entity(primitive).insert((
                MeshMaterial3d(handle.clone()),
                ProjectedPrefabTextureReplacementMaterial(handle),
            ));
        }
    }
}
