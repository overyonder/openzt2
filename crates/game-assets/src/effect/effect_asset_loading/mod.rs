//! Bevy asset loading for authored PSYS particle-effect documents.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy::prelude::*;
use openzt2_game_data::particle::ParticleEffectDocument as ParticleDocument;

use super::{source, ParticleEffectDocumentAsset};
use crate::{
    graphics_settings_types::GraphicsEffectQuality::{High, Low},
    material::{
        material_asset_types::MaterialAsset, runtime::effect_pass_gpu_data::EffectPassMaterial,
    },
};

#[derive(Default, TypePath)]
pub(super) struct EffectAssetLoader;

impl AssetLoader for EffectAssetLoader {
    type Asset = ParticleEffectDocumentAsset;
    type Settings = ();
    type Error = io::Error;

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "load_particle_effect", skip_all)
    )]
    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut particle_system_source_bytes = Vec::new();
        reader
            .read_to_end(&mut particle_system_source_bytes)
            .await?;
        let particle_system_asset_path = load_context.path().path().to_string_lossy();
        let particle_document = source::lower_authored_particle_system_source_to_effect_document(
            &particle_system_asset_path,
            &particle_system_source_bytes,
        )
        .map_err(|source_conversion_error| {
            io::Error::new(io::ErrorKind::InvalidData, source_conversion_error)
        })?;

        let paths: BTreeSet<_> = particle_document
            .emitters
            .iter()
            .flat_map(|emitter| {
                emitter
                    .renderers
                    .iter()
                    .map(|renderer| renderer.material.clone())
            })
            .collect();
        let mut blend_modes = BTreeMap::new();
        for path in paths {
            let material = load_context
                .load_builder()
                .load_value::<MaterialAsset>(path.clone())
                .await
                .map_err(io::Error::other)?;
            let selected_blend = |quality| {
                let [pass] = material
                    .get()
                    .evaluated_pass_material_assets_for_effect_quality(quality)
                else {
                    return None;
                };
                material
                    .get_labeled_by_id(pass)?
                    .get::<EffectPassMaterial>()?
                    .hanabi_blend_mode()
            };
            if let Some(mode) =
                selected_blend(High).filter(|&mode| selected_blend(Low) == Some(mode))
            {
                blend_modes.insert(path, mode);
            }
        }
        Ok(
            super::prepared_effect_emitters::prepare_particle_effect_document(
                ParticleDocument::Effect(particle_document),
                "particle",
                None,
                &blend_modes,
                load_context,
            ),
        )
    }

    fn extensions(&self) -> &[&str] {
        &["psys"]
    }
}
