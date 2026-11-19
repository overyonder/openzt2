use bevy::asset::AssetId;
use bevy::ecs::entity::EntityHashSet;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use openzt2_world_state::model_render::ModelExpanded;

use super::authored_model_material_pass_projection::AuthoredModelStandardMaterial;
use super::configured_model_effect_materials::ModelMaterialInputOwner;
use super::mirrored_model_primitive_winding::InheritedPrimitiveTint;

pub(super) type TintedStandardMaterialKey = (AssetId<StandardMaterial>, [u32; 3]);

pub(super) fn project_model_tints_onto_standard_materials(
    mut commands: Commands,
    mut primitives: Query<(
        Entity,
        Mut<MeshMaterial3d<StandardMaterial>>,
        Option<&mut AuthoredModelStandardMaterial>,
        Option<Ref<InheritedPrimitiveTint>>,
        Option<Ref<ModelMaterialInputOwner<ModelExpanded>>>,
    )>,
    mut removed_tints: RemovedComponents<InheritedPrimitiveTint>,
    mut removed_models: RemovedComponents<ModelMaterialInputOwner<ModelExpanded>>,
    mut assets: ResMut<Assets<StandardMaterial>>,
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut variants: Local<HashMap<TintedStandardMaterialKey, AssetId<StandardMaterial>>>,
) {
    let changed_assets = events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect::<HashSet<_>>();
    if !changed_assets.is_empty() {
        variants.retain(|(source, _), output| {
            !changed_assets.contains(source) && assets.contains(*output)
        });
    }
    let removed = removed_tints
        .read()
        .chain(removed_models.read())
        .collect::<EntityHashSet>();
    for (entity, mut binding, authored, tint, model) in &mut primitives {
        if tint.is_none() && authored.is_none() {
            continue;
        }
        let source = authored
            .as_ref()
            .filter(|authored| authored.rendered_material == binding.id())
            .map_or_else(|| binding.0.clone(), |authored| authored.source.clone());
        if !binding.is_changed()
            && !changed_assets.contains(&source.id())
            && !removed.contains(&entity)
            && !model.as_ref().is_some_and(|model| model.is_changed())
            && !tint.as_ref().is_some_and(|tint| tint.is_changed())
        {
            continue;
        }
        let tint = tint.filter(|tint| tint.0 != Color::WHITE && model.is_some());
        let Some(tint) = tint else {
            if authored.is_some() {
                binding.set_if_neq(MeshMaterial3d(source));
                commands
                    .entity(entity)
                    .remove::<AuthoredModelStandardMaterial>();
            }
            continue;
        };
        let tint = tint.0.to_srgba();
        let key = (
            source.id(),
            [tint.red, tint.green, tint.blue].map(f32::to_bits),
        );
        let target = if let Some(target) = variants
            .get(&key)
            .and_then(|&id| assets.get_strong_handle(id))
        {
            target
        } else {
            let Some(mut material) = assets.get(&source).cloned() else {
                continue;
            };
            let color = material.base_color.to_srgba();
            material.base_color = Color::srgba(
                color.red * tint.red,
                color.green * tint.green,
                color.blue * tint.blue,
                color.alpha,
            );
            let target = assets.add(material);
            variants.insert(key, target.id());
            target
        };
        if let Some(mut authored) = authored {
            if authored.source != source || authored.rendered_material != target.id() {
                authored.source = source;
                authored.rendered_material = target.id();
            }
        } else {
            commands
                .entity(entity)
                .insert(AuthoredModelStandardMaterial {
                    source,
                    rendered_material: target.id(),
                });
        }
        binding.set_if_neq(MeshMaterial3d(target));
    }
}
