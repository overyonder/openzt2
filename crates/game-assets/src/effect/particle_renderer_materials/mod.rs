use bevy::{platform::collections::HashSet, prelude::*};
use bevy_hanabi::{EffectMaterial, EffectRenderPass, EffectRenderPassState, EffectRenderPasses};

use crate::{
    graphics_settings_types::{GraphicsEffectQuality, GraphicsSettings},
    material::{
        material_asset_types::MaterialAsset, runtime::effect_pass_gpu_data::EffectPassMaterial,
    },
};

use super::{
    particle_wave_surface::ParticleEffectWaveSurface, LiveParticleEffectInstance,
    ParticleEffectDocumentAsset,
};

pub(super) fn project_particle_renderer_materials(
    mut commands: Commands,
    documents: Res<Assets<ParticleEffectDocumentAsset>>,
    materials: Res<Assets<MaterialAsset>>,
    passes: Res<Assets<EffectPassMaterial>>,
    graphics: Option<Res<GraphicsSettings>>,
    mut material_events: MessageReader<AssetEvent<MaterialAsset>>,
    mut pass_events: MessageReader<AssetEvent<EffectPassMaterial>>,
    mut emitters: Query<(
        Entity,
        Ref<LiveParticleEffectInstance>,
        Has<ParticleEffectWaveSurface>,
        Option<&mut EffectRenderPasses>,
        Option<&mut EffectMaterial>,
    )>,
) {
    let quality_changed = graphics
        .as_ref()
        .is_some_and(|settings| settings.is_changed());
    let changed_materials: HashSet<_> = material_events.read().filter_map(changed_asset).collect();
    let changed_passes: HashSet<_> = pass_events.read().filter_map(changed_asset).collect();
    let quality = graphics
        .as_ref()
        .map_or(GraphicsEffectQuality::High, |settings| settings.effects);
    for (entity, emitter, wave, existing_draws, existing_material) in &mut emitters {
        let refresh = quality_changed || emitter.is_changed();
        if !refresh && changed_materials.is_empty() && changed_passes.is_empty() {
            continue;
        }
        let Some(prepared) = documents
            .get(&emitter.effect_asset)
            .and_then(|document| document.prepared_emitters.get(&emitter.emitter_id))
        else {
            continue;
        };
        if !refresh
            && !prepared.renderers.iter().any(|renderer| {
                changed_materials.contains(&renderer.material.id())
                    || (!changed_passes.is_empty()
                        && materials.get(&renderer.material).is_some_and(|material| {
                            material
                                .evaluated_pass_material_assets_for_effect_quality(quality)
                                .iter()
                                .any(|pass| changed_passes.contains(&pass.id()))
                        }))
            })
        {
            continue;
        }
        if prepared.renderers.is_empty() {
            if existing_draws.is_some() {
                commands
                    .entity(entity)
                    .remove::<(EffectRenderPasses, EffectMaterial)>();
            }
            continue;
        }

        let mut draws = Vec::new();
        let mut simulation_textures = Vec::new();
        for (ordinal, renderer) in prepared.renderers.iter().enumerate() {
            let Some(material) = materials.get(&renderer.material) else {
                continue;
            };
            let is_wave = ordinal == 0 && wave;
            let effect = if is_wave {
                prepared.wave_effect.as_ref().unwrap_or(&renderer.effect)
            } else {
                &renderer.effect
            };
            let pass_handles = material.evaluated_pass_material_assets_for_effect_quality(quality);
            for pass in pass_handles
                .iter()
                .map(Some)
                .chain(pass_handles.is_empty().then_some(None))
            {
                let pass = match pass {
                    Some(handle) => {
                        let Some(pass) = passes.get(handle) else {
                            continue;
                        };
                        Some(pass)
                    }
                    None => None,
                };
                let textures = if is_wave {
                    ["Texture0", "Texture1"]
                        .map(|name| {
                            material
                                .bound_texture_asset(name)
                                .cloned()
                                .unwrap_or_default()
                        })
                        .to_vec()
                } else {
                    vec![pass
                        .and_then(EffectPassMaterial::bound_particle_texture)
                        .or_else(|| material.bound_texture_asset("Texture0"))
                        .or_else(|| material.first_bound_texture_asset())
                        .cloned()
                        .unwrap_or_default()]
                };
                if ordinal == 0 && simulation_textures.is_empty() {
                    simulation_textures.clone_from(&textures);
                }
                let draw_state = pass.map(EffectPassMaterial::draw_state);
                let stencil_reference = draw_state
                    .as_ref()
                    .and_then(|state| state.stencil_reference);
                draws.push(EffectRenderPass {
                    effect: effect.clone(),
                    textures,
                    sort_by_distance: renderer.sort_by_distance,
                    state: draw_state.map(|state| EffectRenderPassState {
                        blend: state.blend,
                        depth_write_enabled: state.depth_write_enabled,
                        depth_compare: state.depth_compare,
                        stencil: state.stencil,
                        color_writes: state.color_writes,
                    }),
                    stencil_reference,
                });
            }
        }
        let draws = EffectRenderPasses(draws);
        if let Some(mut existing) = existing_draws {
            existing.set_if_neq(draws);
        } else {
            commands.entity(entity).insert(draws);
        }
        if let Some(mut existing) = existing_material {
            if existing.images != simulation_textures {
                existing.images = simulation_textures;
            }
        } else if !simulation_textures.is_empty() {
            commands.entity(entity).insert(EffectMaterial {
                images: simulation_textures,
            });
        }
    }
}

fn changed_asset<A: Asset>(event: &AssetEvent<A>) -> Option<bevy::asset::AssetId<A>> {
    match *event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::Removed { id }
        | AssetEvent::LoadedWithDependencies { id } => Some(id),
        AssetEvent::Unused { .. } => None,
    }
}
