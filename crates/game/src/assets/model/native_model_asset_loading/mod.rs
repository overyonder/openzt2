//! Loads NIF and BFB models as glTF assets and labelled subassets.

use std::io;

use bevy::{
    asset::{io::Reader, AssetApp, AssetLoader, LoadContext},
    gltf::{Gltf, GltfLoader, GltfLoaderSettings},
    prelude::*,
};

use crate::{
    asset_source::AssetArchives,
    assets::{
        effect::ParticleEffectDocumentAsset,
        material::evaluated_effect_material_asset_construction::create_material_asset_from_evaluated_d3d9_effect,
        model::bevy_gltf_loader_configuration::create_bevy_gltf_loader_for_lowered_source_assets,
        model_source::native_model_source_lowering::lower_native_model_source, scene_prefab,
    },
};

const SCENE_LABEL: &str = "Scene";

#[derive(TypePath)]
struct NativeModelAssetLoader {
    bevy_gltf_loader: GltfLoader,
    archives: AssetArchives,
}

impl FromWorld for NativeModelAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            bevy_gltf_loader: create_bevy_gltf_loader_for_lowered_source_assets(world),
            archives: world.resource::<AssetArchives>().clone(),
        }
    }
}

impl AssetLoader for NativeModelAssetLoader {
    type Asset = Gltf;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        native_model_source_reader: &mut dyn Reader,
        _: &Self::Settings,
        native_model_load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let _performance_timer = self
            .archives
            .measure_scene_loading_asset_translation("native_model");
        let mut native_model_source_bytes = Vec::new();
        native_model_source_reader
            .read_to_end(&mut native_model_source_bytes)
            .await?;
        let native_model_asset_path = native_model_load_context
            .path()
            .path()
            .to_string_lossy()
            .into_owned();
        let lowered_native_model = lower_native_model_source(
            &native_model_asset_path,
            &native_model_source_bytes,
            |texture_reference| {
                self.archives
                    .resolve_model_texture_reference(
                        native_model_load_context.path().path(),
                        texture_reference,
                    )
                    .map(|resolved| resolved.to_string_lossy().into_owned())
            },
        )
        .map_err(io::Error::other)?;
        let loaded_gltf = GltfLoader::load_gltf(
            &self.bevy_gltf_loader,
            &lowered_native_model.glb,
            native_model_load_context,
            &GltfLoaderSettings::default(),
        )
        .await
        .map_err(io::Error::other)?;

        for lowered_material in lowered_native_model.materials {
            let label = labelled_native_model_subasset_label(&lowered_material.labelled_asset_path)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid NIF material path")
                })?;
            let evaluated_pass_label_prefix = format!("{label}/effect-pass");
            let material = create_material_asset_from_evaluated_d3d9_effect(
                lowered_material.evaluated_d3d9_effect,
                lowered_material.texture_asset_paths,
                &evaluated_pass_label_prefix,
                native_model_load_context,
                &lowered_material.texture_coordinate_animations,
            )?;
            native_model_load_context.add_labeled_asset(label, material);
        }

        native_model_load_context.labeled_asset_scope(SCENE_LABEL, |label_context| {
            Ok::<_, io::Error>(
                scene_prefab::create_scene_prefab_asset_and_load_dependencies(
                    lowered_native_model.scene_prefab_document,
                    label_context,
                ),
            )
        })?;

        for lowered_particle_effect in lowered_native_model.particle_effects {
            let label =
                labelled_native_model_subasset_label(&lowered_particle_effect.labelled_asset_path)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid NIF effect path")
                    })?;
            native_model_load_context.add_labeled_asset(
                label,
                ParticleEffectDocumentAsset {
                    particle_document: lowered_particle_effect.particle_effect_document,
                },
            );
        }
        Ok(loaded_gltf)
    }

    fn extensions(&self) -> &[&str] {
        &["nif", "bfb"]
    }
}

fn labelled_native_model_subasset_label(path: &str) -> Option<String> {
    path.split_once('#').map(|(_, label)| label.to_owned())
}

pub(in crate::assets) struct NativeModelAssetPlugin;

impl Plugin for NativeModelAssetPlugin {
    fn build(&self, application: &mut App) {
        application.preregister_asset_loader::<NativeModelAssetLoader>(&["nif", "bfb"]);
    }

    fn finish(&self, application: &mut App) {
        let native_model_asset_loader = NativeModelAssetLoader::from_world(application.world_mut());
        application.register_asset_loader(native_model_asset_loader);
    }
}
