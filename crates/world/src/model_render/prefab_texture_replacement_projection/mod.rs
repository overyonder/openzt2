//! Binds an entity's drawn texture replacements onto its projected effect passes.

use bevy::prelude::*;

use openzt2_game_assets::graphics_settings_types::GraphicsSettings;

use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::prefab_texture_replacements::PrefabTextureReplacements;

/// The replaced material this primitive currently draws with, and the
/// material it drew with before, restored when replacements are switched off.
#[derive(Component)]
pub(super) struct ProjectedPrefabTextureReplacementMaterial {
    replaced: Handle<EffectPassMaterial>,
    original: Handle<EffectPassMaterial>,
}

/// Projects an owner's whole tree when its replacements change or the
/// texture detail setting does, and otherwise only primitives whose material
/// changed, such as a head that loads after its body: every guest owns
/// replacements, so the trees are not walked each frame.
#[allow(clippy::type_complexity)]
pub(super) fn project_prefab_texture_replacements_onto_effect_pass_materials(
    mut commands: Commands,
    owners: Query<(Entity, Ref<PrefabTextureReplacements>)>,
    changed_primitives: Query<Entity, Changed<MeshMaterial3d<EffectPassMaterial>>>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    models: Query<&PrefabModel>,
    primitives: Query<(
        &MeshMaterial3d<EffectPassMaterial>,
        Option<&ProjectedPrefabTextureReplacementMaterial>,
    )>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    graphics: Option<Res<GraphicsSettings>>,
) {
    let setting_changed = graphics
        .as_ref()
        .is_some_and(|graphics| graphics.is_changed());
    // Low object texture detail authors `textureReplaceOn` false.
    if graphics.is_some_and(|graphics| !graphics.texture_replacement) {
        if !setting_changed {
            return;
        }
        for (owner, _) in &owners {
            for primitive in children.iter_descendants_depth_first::<Children>(owner) {
                if let Ok((_, Some(projected))) = primitives.get(primitive) {
                    commands
                        .entity(primitive)
                        .insert(MeshMaterial3d(projected.original.clone()));
                    commands
                        .entity(primitive)
                        .remove::<ProjectedPrefabTextureReplacementMaterial>();
                }
            }
        }
        return;
    }
    let mut project = |primitive: Entity, replacements: &PrefabTextureReplacements, again: bool| {
        let Ok((current, projected)) = primitives.get(primitive) else {
            return;
        };
        if !again && projected.is_some_and(|projected| projected.replaced == current.0) {
            return;
        }
        // Changed replacements start again from the material before the last
        // ones; a material another system swapped in is the new original.
        let original = match projected {
            Some(projected) if again => &projected.original,
            _ => &current.0,
        };
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
            return;
        };
        let Some(mut material) = materials.get(original).cloned() else {
            return;
        };
        // Native NIF materials bind their base map at stage 0.
        material.replace_texture_asset_at_d3d9_stage(0, texture.clone());
        let handle = materials.add(material);
        commands.entity(primitive).insert((
            MeshMaterial3d(handle.clone()),
            ProjectedPrefabTextureReplacementMaterial {
                replaced: handle,
                original: original.clone(),
            },
        ));
    };
    for (owner, replacements) in &owners {
        if setting_changed || replacements.is_changed() {
            for primitive in children.iter_descendants_depth_first::<Children>(owner) {
                project(primitive, &replacements, replacements.is_changed());
            }
        }
    }
    for primitive in &changed_primitives {
        if let Some((_, replacements)) = parents
            .iter_ancestors::<ChildOf>(primitive)
            .find_map(|entity| owners.get(entity).ok())
        {
            project(primitive, &replacements, false);
        }
    }
}
