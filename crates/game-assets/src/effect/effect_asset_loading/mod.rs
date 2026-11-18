//! Bevy asset loading for authored PSYS particle-effect documents.

use std::io;

use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy::prelude::*;
use openzt2_game_data::particle::ParticleEffectDocument as ParticleDocument;

use super::{source, ParticleEffectDocumentAsset};
use crate::material::material_asset_types::MaterialAsset;

#[derive(Default, TypePath)]
pub(super) struct EffectAssetLoader;

impl AssetLoader for EffectAssetLoader {
    type Asset = ParticleEffectDocumentAsset;
    type Settings = ();
    type Error = io::Error;

    #[cfg_attr(feature = "tracing", tracing::instrument(name = "load_particle_effect", skip_all))]
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
        .map(ParticleDocument::Effect)
        .map_err(|source_conversion_error| {
            io::Error::new(io::ErrorKind::InvalidData, source_conversion_error)
        })?;

        match &particle_document {
            ParticleDocument::Effect(effect_document) => {
                effect_document.emitters.iter().for_each(|effect_emitter| {
                    let _: Handle<MaterialAsset> =
                        load_context.load(effect_emitter.material.clone());
                });
            }
            ParticleDocument::LegacyNif(legacy_nif_particle_effect) => {
                let _: Handle<bevy::gltf::Gltf> =
                    load_context.load(legacy_nif_particle_effect.model.clone());
            }
        }

        Ok(ParticleEffectDocumentAsset { particle_document })
    }

    fn extensions(&self) -> &[&str] {
        &["psys"]
    }
}
