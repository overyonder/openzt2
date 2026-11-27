use std::{
    io,
    path::{Path, PathBuf},
};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_data::{
    scene_prefab::{PrefabAnimationStart, PrefabTerrainDecal, ScenePrefabDocument},
    AssetId,
};

use crate::{
    asset_source::AssetArchives,
    model_source::{
        native_model_source_lowering::lower_native_physical_scene_document,
        native_scene::scene_prefab_document_assembly::authored_terrain_decal_row,
    },
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
    },
    texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image,
};

use super::{
    create_scene_prefab_asset_and_load_dependencies, ScenePrefabAsset, ScenePrefabTerrainDecal,
};

#[derive(TypePath)]
pub(super) struct AuthoredPhysicalSceneLoader(AssetArchives);

impl FromWorld for AuthoredPhysicalSceneLoader {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource::<AssetArchives>().clone())
    }
}

impl AssetLoader for AuthoredPhysicalSceneLoader {
    type Asset = ScenePrefabAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        context: &mut LoadContext<'_>,
    ) -> io::Result<ScenePrefabAsset> {
        let path = context.path().path().to_owned();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source =
            parse_blue_fang_source_document(AssetPath::new(path.to_string_lossy()), &bytes)
                .map_err(io::Error::other)?;
        if !source.root.name.eq_ignore_ascii_case("BFPhysObj") {
            return Err(invalid("scene source must contain a BFPhysObj"));
        }
        let root = &source.root;
        let scene = child(root, "BFSceneGraphComponent");
        let actor = child(root, "BFActorComponent");
        let ground_fit = child(root, "BFGroundFitComponent");
        let actor_path = actor
            .and_then(|node| node.attribute_named_any(&["actorfile"]))
            .map(|reference| {
                self.0
                    .resolve_source_reference(&path, reference)
                    .ok_or_else(|| invalid(format!("unresolved actor {reference:?}")))
            })
            .transpose()?;
        let model = if let Some(actor_path) = actor_path.as_ref() {
            let bytes = context
                .read_asset_bytes(actor_path.clone())
                .await
                .map_err(io::Error::other)?;
            let source = actor_path.to_string_lossy();
            let actor = if actor_path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("kfm"))
            {
                crate::animation::kfm_animation_set_lowering::lower_kfm_source_to_animation_set_document(&source, &bytes)?
            } else {
                crate::animation::bfm_animation_set_lowering::lower_bfm_source_to_animation_set_document(&source, &bytes)?
            };
            Some(self.model_path(actor_path, &actor.model_asset_path)?)
        } else {
            scene
                .and_then(|node| node.attribute_named_any(&["modelfile"]))
                .map(|reference| self.model_path(&path, reference))
                .transpose()?
        };
        let mut document = if let Some(model) = model {
            let bytes = context
                .read_asset_bytes(model.clone())
                .await
                .map_err(io::Error::other)?;
            lower_native_physical_scene_document(&model.to_string_lossy(), &bytes)
                .map_err(io::Error::other)?
        } else {
            ScenePrefabDocument {
                entities: Vec::new(),
                automatic_placement_bounds_xz: None,
                rail_camera: None,
                dependencies: Vec::new(),
            }
        };
        let scale = scene
            .or(actor)
            .map(|node| number(node, "scale", 1.0))
            .transpose()?
            .unwrap_or(1.0);
        for row in document
            .entities
            .iter_mut()
            .filter(|row| row.parent == u32::MAX)
        {
            row.transform.scale = row.transform.scale.map(|axis| axis * scale);
            row.transform.translation_m = row.transform.translation_m.map(|axis| axis * scale);
        }
        if let Some(bounds) = &mut document.automatic_placement_bounds_xz {
            *bounds = bounds.map(|point| point.map(|axis| axis * scale));
        }
        let terrain_decal = child(root, "BFTerrainDecalComponent")
            .map(|node| self.terrain_decal(node, &path, context))
            .transpose()?;
        let decal_texture = terrain_decal.map(|(decal, texture)| {
            document
                .entities
                .push(authored_terrain_decal_row(&path.to_string_lossy(), decal));
            texture
        });
        let mut asset = create_scene_prefab_asset_and_load_dependencies(document, context);
        asset.ground_fit_height_offset_m = ground_fit
            .map(|node| number(node, "heightOffset", 0.0))
            .transpose()?;
        asset.terrain_decal = decal_texture;
        if let Some(path) = actor_path {
            let handle = context.load(path);
            let start = actor.map(animation_start).transpose()?.flatten();
            for row in 0..asset.document.entities.len() {
                for renderable in 0..asset.document.entities[row].renderables.len() {
                    asset.bind_animation_set_to_renderable(
                        row,
                        renderable,
                        handle.clone(),
                        start.clone(),
                    );
                }
            }
        }
        Ok(asset)
    }
}
impl AuthoredPhysicalSceneLoader {
    fn model_path(&self, from: &Path, reference: &str) -> io::Result<PathBuf> {
        self.0
            .resolve_model_reference(from, reference)
            .ok_or_else(|| invalid(format!("unresolved scene model {reference:?}")))
    }

    fn terrain_decal(
        &self,
        node: &OrderedSourceDocumentNode,
        from: &Path,
        context: &mut LoadContext<'_>,
    ) -> io::Result<(PrefabTerrainDecal, ScenePrefabTerrainDecal)> {
        let reference = node
            .attribute_named_any(&["imageName"])
            .ok_or_else(|| invalid("terrain decal has no imageName"))?;
        let texture = self
            .0
            .resolve_model_texture_reference(from, reference)
            .ok_or_else(|| invalid(format!("unresolved terrain decal image {reference:?}")))?;
        let texture = texture.to_string_lossy();
        let radius_m = number(node, "radius", 0.0)?;
        if radius_m <= 0.0 {
            return Err(invalid("terrain decal radius must be positive"));
        }
        let mut lighting = [None; 4];
        if let Some(source) = child(node, "lighting") {
            for (color, field) in lighting.iter_mut().zip([
                "ambientColor",
                "diffuseColor",
                "specularColor",
                "emissiveColor",
            ]) {
                if let Some(value) = source.attribute_named_any(&[field]) {
                    let channels = value
                        .split_ascii_whitespace()
                        .map(str::parse::<u8>)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(io::Error::other)?;
                    *color = Some(
                        channels
                            .try_into()
                            .map_err(|_| invalid(format!("invalid terrain decal {field}")))?,
                    );
                }
            }
        }
        let id = AssetId::from_virtual_path(&texture);
        Ok((
            PrefabTerrainDecal {
                texture: id,
                radius_metres: radius_m,
                alpha_blend: boolean(node, "alphaBlend", false)?,
                lighting: lighting[3] != Some([255; 3]),
            },
            ScenePrefabTerrainDecal {
                texture: id,
                lighting_colors: lighting,
                image: context.load(select_bevy_image_asset_path_for_blue_fang_source_image(
                    &texture,
                )),
            },
        ))
    }
}

pub(crate) fn animation_start(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<PrefabAnimationStart>> {
    node.attribute_named_any(&["activeSequenceName"])
        .filter(|key| !key.trim().is_empty())
        .map(|key| {
            Ok(PrefabAnimationStart {
                clip_key: key.to_owned(),
                looping: boolean(node, "activeSequenceLoop", true)?,
                time_seconds: number(node, "activeSequenceTime", 0.0)?,
            })
        })
        .transpose()
}

fn child<'a>(
    node: &'a OrderedSourceDocumentNode,
    tag: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    node.element_children()
        .find(|child| child.name.eq_ignore_ascii_case(tag))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn number(node: &OrderedSourceDocumentNode, attribute: &str, default: f32) -> io::Result<f32> {
    let Some(value) = node.attribute_named_any(&[attribute]) else {
        return Ok(default);
    };
    value
        .trim()
        .trim_end_matches(['f', 'F'])
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid(format!("invalid {attribute} value {value:?}")))
}

fn boolean(node: &OrderedSourceDocumentNode, attribute: &str, default: bool) -> io::Result<bool> {
    let Some(value) = node.attribute_named_any(&[attribute]) else {
        return Ok(default);
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(invalid(format!("invalid {attribute} value {value:?}"))),
    }
}
