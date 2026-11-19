use bevy::prelude::*;
use bevy_hanabi::{EffectAsset, EffectMaterial};

use crate::{
    graphics_settings_types::{GraphicsEffectQuality, GraphicsSettings},
    material::{
        material_asset_types::MaterialAsset, runtime::effect_pass_gpu_data::EffectPassMaterial,
    },
};

use super::{particle_wave_surface::ParticleEffectWaveSurface, ParticleEffectDocumentAsset};

/// Draw-only assets for an emitter. Every renderer uses the emitter's one
/// particle population. The first shader also supplies the built-in draw.
#[derive(Component, Debug, PartialEq)]
pub struct AuthoredParticleRenderers(pub Box<[AuthoredParticleDraw]>);

#[derive(Debug, PartialEq)]
pub struct AuthoredParticleDraw {
    pub effect: Handle<EffectAsset>,
    pub passes: Box<[Handle<EffectPassMaterial>]>,
    pub sort_by_distance: bool,
}

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
        Ref<super::LiveParticleEffectInstance>,
        Has<ParticleEffectWaveSurface>,
        Option<&mut AuthoredParticleRenderers>,
        Option<&mut EffectMaterial>,
    )>,
) {
    let refresh_all = graphics
        .as_ref()
        .is_some_and(|graphics| graphics.is_changed())
        | (material_events.read().count() != 0)
        | (pass_events.read().count() != 0);
    let quality = graphics
        .as_ref()
        .map_or(GraphicsEffectQuality::High, |graphics| graphics.effects);
    for (entity, emitter, wave, existing_draws, existing_images) in &mut emitters {
        if !refresh_all && !emitter.is_changed() {
            continue;
        }
        let Some(prepared) = documents
            .get(&emitter.effect_asset)
            .and_then(|doc| doc.prepared_emitters.get(&emitter.emitter_id))
        else {
            continue;
        };
        if prepared.renderers.is_empty() {
            continue;
        }
        let mut images = Vec::new();
        let mut draws = Vec::with_capacity(prepared.renderers.len());
        for (ordinal, renderer) in prepared.renderers.iter().enumerate() {
            let Some(material) = materials.get(&renderer.material) else {
                continue;
            };
            let pass_handles = material.evaluated_pass_material_assets_for_effect_quality(quality);
            if ordinal == 0 {
                if wave {
                    if let (Some(ripple), Some(mask)) = (
                        material.bound_texture_asset("Texture0"),
                        material.bound_texture_asset("Texture1"),
                    ) {
                        images.extend([ripple.clone(), mask.clone()]);
                    }
                } else {
                    let texture = pass_handles
                        .first()
                        .and_then(|handle| passes.get(handle))
                        .and_then(EffectPassMaterial::bound_particle_texture)
                        .or_else(|| material.bound_texture_asset("Texture0"))
                        .or_else(|| material.first_bound_texture_asset());
                    images.push(texture.cloned().unwrap_or_default());
                }
            }
            let effect = if ordinal == 0 && wave {
                prepared.wave_effect.as_ref().unwrap_or(&renderer.effect)
            } else {
                &renderer.effect
            };
            draws.push(AuthoredParticleDraw {
                effect: effect.clone(),
                passes: pass_handles.into(),
                sort_by_distance: renderer.sort_by_distance,
            });
        }
        let draws = AuthoredParticleRenderers(draws.into_boxed_slice());
        if let Some(mut existing) = existing_draws {
            existing.set_if_neq(draws);
        } else {
            commands.entity(entity).insert(draws);
        }
        if let Some(mut existing) = existing_images {
            if existing.images != images {
                existing.images = images;
            }
        } else if !images.is_empty() {
            commands.entity(entity).insert(EffectMaterial { images });
        }
    }
}
