use bevy::asset::{asset_changed::AssetChanged, AssetId};
use bevy::color::Alpha;
use bevy::ecs::entity::EntityHashSet;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use openzt2_world_state::model_render::ModelExpanded;

use super::authored_model_material_pass_projection::AuthoredModelStandardMaterial;
use super::configured_model_effect_materials::ModelMaterialInputOwner;
use super::mirrored_model_primitive_winding::InheritedPrimitiveTint;
use crate::world_spawn::prefab_world_instance_spawning::morph_fade_projection::{
    projected_prefab_opacity, NamedPrefabOpacity, PrefabMorphOpacity,
};

pub(super) type TintedStandardMaterialKey = (AssetId<StandardMaterial>, [u32; 3]);

pub(super) fn project_model_tints_onto_standard_materials(
    mut commands: Commands,
    mut primitives: ParamSet<(
        Query<(
            Entity,
            Mut<MeshMaterial3d<StandardMaterial>>,
            Option<&mut AuthoredModelStandardMaterial>,
            Option<Ref<InheritedPrimitiveTint>>,
            Option<Ref<ModelMaterialInputOwner<ModelExpanded>>>,
            Option<&PrefabMorphOpacity>,
            Option<&NamedPrefabOpacity>,
        )>,
        Query<
            Entity,
            (
                With<MeshMaterial3d<StandardMaterial>>,
                Or<(
                    Changed<MeshMaterial3d<StandardMaterial>>,
                    AssetChanged<MeshMaterial3d<StandardMaterial>>,
                    AssetChanged<AuthoredModelStandardMaterial>,
                    Changed<InheritedPrimitiveTint>,
                    Changed<ModelMaterialInputOwner<ModelExpanded>>,
                    Changed<PrefabMorphOpacity>,
                    Changed<NamedPrefabOpacity>,
                )>,
            ),
        >,
    )>,
    mut removed_tints: RemovedComponents<InheritedPrimitiveTint>,
    mut removed_models: RemovedComponents<ModelMaterialInputOwner<ModelExpanded>>,
    mut removed_morphs: RemovedComponents<PrefabMorphOpacity>,
    mut removed_named: RemovedComponents<NamedPrefabOpacity>,
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
    let changed = primitives
        .p1()
        .iter()
        .chain(removed_tints.read().chain(removed_models.read()))
        .chain(removed_morphs.read())
        .chain(removed_named.read())
        .collect::<EntityHashSet>();
    let mut primitives = primitives.p0();
    let mut primitives = primitives.iter_many_mut(changed).matched();
    while let Some((entity, mut binding, authored, tint, model, morph, named)) =
        primitives.fetch_next()
    {
        let previous = authored
            .as_ref()
            .filter(|authored| authored.rendered_material == binding.id());
        let source = previous.map_or_else(|| binding.0.clone(), |authored| authored.source.clone());
        let tint = tint
            .filter(|tint| tint.0 != Color::WHITE && model.is_some())
            .map(|tint| tint.0.to_srgba());
        let opacity = projected_prefab_opacity(morph, named).filter(|&value| value != 1.0);
        if tint.is_none() && opacity.is_none() {
            if authored.is_some() {
                binding.set_if_neq(MeshMaterial3d(source));
                commands
                    .entity(entity)
                    .remove::<AuthoredModelStandardMaterial>();
            }
            continue;
        }
        let key = tint.map(|tint| {
            (
                source.id(),
                [tint.red, tint.green, tint.blue].map(f32::to_bits),
            )
        });
        let shared = key.filter(|_| opacity.is_none());
        let target = if let Some(handle) = shared
            .and_then(|key| variants.get(&key))
            .and_then(|&id| assets.get_strong_handle(id))
        {
            handle
        } else {
            let Some(mut material) = assets.get(&source).cloned() else {
                continue;
            };
            if let Some(tint) = tint {
                let color = material.base_color.to_srgba();
                material.base_color = Color::srgba(
                    color.red * tint.red,
                    color.green * tint.green,
                    color.blue * tint.blue,
                    color.alpha,
                );
            }
            if let Some(opacity) = opacity {
                material.alpha_mode = AlphaMode::Blend;
                material
                    .base_color
                    .set_alpha(material.base_color.alpha() * opacity);
            }
            if opacity.is_some()
                && previous.is_some_and(|previous| previous.fading)
                && assets.contains(binding.id())
            {
                let id = binding.id();
                if changed_assets.contains(&source.id()) {
                    if let Some(mut current) = assets.get_mut(id) {
                        *current = material;
                    }
                } else if assets
                    .get(id)
                    .is_some_and(|current| current.base_color != material.base_color)
                {
                    if let Some(mut current) = assets.get_mut(id) {
                        current.base_color = material.base_color;
                    }
                }
                binding.0.clone()
            } else {
                let target = assets.add(material);
                if let Some(key) = shared {
                    variants.insert(key, target.id());
                }
                target
            }
        };
        if let Some(mut authored) = authored {
            if authored.source != source
                || authored.rendered_material != target.id()
                || authored.fading != opacity.is_some()
            {
                authored.source = source;
                authored.rendered_material = target.id();
                authored.fading = opacity.is_some();
            }
        } else {
            commands
                .entity(entity)
                .insert(AuthoredModelStandardMaterial {
                    source,
                    rendered_material: target.id(),
                    fading: opacity.is_some(),
                });
        }
        binding.set_if_neq(MeshMaterial3d(target));
    }
}
