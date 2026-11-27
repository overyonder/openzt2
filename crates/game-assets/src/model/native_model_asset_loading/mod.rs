//! Loads NIF, BFB and mod-supplied glTF models as glTF assets and labelled
//! subassets.

use std::io;

use bevy::{
    asset::{io::Reader, AssetApp, AssetLoader, LoadContext},
    gltf::{Gltf, GltfLoader, GltfLoaderSettings},
    prelude::*,
};

use crate::{
    animation::animation_set_asset_types::{AnimationClipRecord, AnimationSetAsset},
    asset_source::AssetArchives,
    {
        effect::prepared_effect_emitters::prepare_particle_effect_document,
        material::evaluated_effect_material_asset_construction::create_material_asset_from_evaluated_d3d9_effect,
        model::bevy_gltf_loader_configuration::create_bevy_gltf_loader_for_lowered_source_assets,
        model_source::{
            gltf_model_source::{
                gltf_model_scene_prefab, is_gltf_model_asset_path,
                replace_scene_prefab_renderables_with_whole_gltf_scene,
            },
            native_model_source_lowering::lower_native_model_source,
            native_model_source_lowering_types::LoweredNativeModel,
        },
        scene_prefab,
    },
};

const SCENE_LABEL: &str = "Scene";
pub const EMBEDDED_ANIMATION_SET_LABEL: &str = "EmbeddedAnimationSet";

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

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "load_native_model", skip_all)
    )]
    async fn load(
        &self,
        native_model_source_reader: &mut dyn Reader,
        _: &Self::Settings,
        native_model_load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut native_model_source_bytes = Vec::new();
        native_model_source_reader
            .read_to_end(&mut native_model_source_bytes)
            .await?;
        let native_model_asset_path = native_model_load_context
            .path()
            .path()
            .to_string_lossy()
            .into_owned();
        let mut model_source = blake3::Hasher::new();
        let lowered_native_model =
            if is_gltf_model_asset_path(native_model_load_context.path().path()) {
                if let Some(replacement) = self
                    .archives
                    .live_model_replacement(native_model_load_context.path().path())
                {
                    native_model_source_bytes = replacement.to_vec();
                }
                model_source.update(&native_model_source_bytes);
                LoweredNativeModel {
                    scene_prefab_document: gltf_model_scene_prefab(
                        &native_model_asset_path,
                        &native_model_source_bytes,
                    )
                    .map_err(io::Error::other)?,
                    glb: native_model_source_bytes,
                    particle_effects: Vec::new(),
                    materials: Vec::new(),
                    embedded_textures: Vec::new(),
                }
            } else {
                let replacement = self
                    .archives
                    .gltf_model_replacement(native_model_load_context.path().path())?;
                model_source.update(&[u8::from(replacement.is_some())]);
                model_source.update(&native_model_source_bytes);
                if let Some(replacement) = &replacement {
                    model_source.update(blake3::hash(replacement).as_bytes());
                }
                let lowered = lower_native_model_source(
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
                );
                match (lowered, replacement) {
                    (Ok(lowered), None) => lowered,
                    (Ok(lowered), Some(replacement)) => LoweredNativeModel {
                        scene_prefab_document:
                            replace_scene_prefab_renderables_with_whole_gltf_scene(
                                lowered.scene_prefab_document,
                                &native_model_asset_path,
                            ),
                        glb: replacement.to_vec(),
                        particle_effects: lowered.particle_effects,
                        materials: Vec::new(),
                        embedded_textures: Vec::new(),
                    },
                    // A replacement still loads when the model it replaces cannot.
                    (Err(_), Some(replacement)) => LoweredNativeModel {
                        scene_prefab_document: gltf_model_scene_prefab(
                            &native_model_asset_path,
                            &replacement,
                        )
                        .map_err(io::Error::other)?,
                        glb: replacement.to_vec(),
                        particle_effects: Vec::new(),
                        materials: Vec::new(),
                        embedded_textures: Vec::new(),
                    },
                    (Err(error), None) => return Err(io::Error::other(error)),
                }
            };
        let loaded_gltf = GltfLoader::load_gltf(
            &self.bevy_gltf_loader,
            &lowered_native_model.glb,
            native_model_load_context,
            &GltfLoaderSettings::default(),
        )
        .await
        .map_err(io::Error::other)?;

        let mut embedded_clips = loaded_gltf
            .animations
            .iter()
            .enumerate()
            .map(|(index, handle)| {
                let clip = native_model_load_context
                    .get_labeled_by_id(handle)
                    .and_then(|asset| asset.get::<AnimationClip>())
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "missing glTF animation subasset",
                        )
                    })?;
                let key = loaded_gltf
                    .named_animations
                    .iter()
                    .find_map(|(name, named)| (named == handle).then(|| name.to_string()))
                    .unwrap_or_else(|| format!("animation_{index}"));
                Ok(AnimationClipRecord::create_embedded_animation_clip_record(
                    key,
                    handle.clone(),
                    (clip.duration() * 1000.0).max(1.0) as u32,
                ))
            })
            .collect::<Result<Vec<_>, io::Error>>()?;
        let embedded_animation_graph = if embedded_clips.is_empty() {
            None
        } else {
            let (graph, nodes) = AnimationGraph::from_clips(
                embedded_clips
                    .iter()
                    .map(|clip| clip.animation_clip_handle.clone()),
            );
            for (clip, node) in embedded_clips.iter_mut().zip(nodes) {
                clip.graph_node = Some(node);
            }
            Some(native_model_load_context.add_labeled_asset("EmbeddedAnimationGraph", graph))
        };
        let mut embedded = AnimationSetAsset::create_ready_animation_set_asset(
            String::new(),
            String::new(),
            embedded_clips,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
        );
        embedded.embedded_animation_graph = embedded_animation_graph;
        native_model_load_context.add_labeled_asset(EMBEDDED_ANIMATION_SET_LABEL, embedded);

        for (label, image) in lowered_native_model.embedded_textures {
            native_model_load_context.add_labeled_asset(label, image);
        }
        for lowered_material in lowered_native_model.materials {
            let label = labelled_native_model_subasset_label(&lowered_material.labelled_asset_path)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid NIF material path")
                })?;
            let evaluated_pass_label_prefix = format!("{label}/effect-pass");
            let mut material = create_material_asset_from_evaluated_d3d9_effect(
                lowered_material.evaluated_d3d9_effect,
                lowered_material.texture_asset_paths,
                &evaluated_pass_label_prefix,
                native_model_load_context,
                &lowered_material.texture_coordinate_animations,
                &lowered_material.reflection_projections,
            )?;
            material.netimmerse_texture_binding = Some(lowered_material.texture_binding);
            material.netimmerse_material_animations = lowered_material.material_animations;
            native_model_load_context.add_labeled_asset(label, material);
        }

        native_model_load_context.labeled_asset_scope(SCENE_LABEL, |label_context| {
            Ok::<_, io::Error>(
                scene_prefab::create_scene_prefab_asset_and_load_dependencies(
                    lowered_native_model.scene_prefab_document,
                    label_context,
                )
                .with_model_source_digest(*model_source.finalize().as_bytes()),
            )
        })?;

        for lowered_particle_effect in lowered_native_model.particle_effects {
            let label =
                labelled_native_model_subasset_label(&lowered_particle_effect.labelled_asset_path)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid NIF effect path")
                    })?;
            let asset = prepare_particle_effect_document(
                lowered_particle_effect.particle_effect_document,
                &label,
                Some(&loaded_gltf),
                &Default::default(),
                native_model_load_context,
            );
            native_model_load_context.add_labeled_asset(label, asset);
        }
        Ok(loaded_gltf)
    }

    fn extensions(&self) -> &[&str] {
        NATIVE_MODEL_EXTENSIONS
    }
}

/// Bevy matches loader extensions case-sensitively, and the shipped archives
/// keep some models as `.NIF` (Lion, Jaguar, LeopardSnow, PenguinEmperor).
/// glTF models from mods arrive under their routed extensions.
const NATIVE_MODEL_EXTENSIONS: &[&str] = &["nif", "bfb", "NIF", "BFB", "z2glb", "z2gltf"];

fn labelled_native_model_subasset_label(path: &str) -> Option<String> {
    path.split_once('#').map(|(_, label)| label.to_owned())
}

pub struct NativeModelAssetPlugin;

impl Plugin for NativeModelAssetPlugin {
    fn build(&self, application: &mut App) {
        application.preregister_asset_loader::<NativeModelAssetLoader>(NATIVE_MODEL_EXTENSIONS);
    }

    fn finish(&self, application: &mut App) {
        let native_model_asset_loader = NativeModelAssetLoader::from_world(application.world_mut());
        application.register_asset_loader(native_model_asset_loader);
    }
}
