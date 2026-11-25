//! A model as one glTF scene that a 3D editor can open and send back as its
//! replacement.

use std::{borrow::Cow, collections::BTreeSet, io::Cursor, path::Path};

use anyhow::{Context, Result};
use gltf::json::{
    buffer::View, image::MimeType, scene::UnitQuaternion, texture::Info, validation::USize64,
    Image, Index, Node, Root, Scene, Texture,
};
use openzt2_game_data::{
    scene_prefab::{ScenePrefabAssetDependencyKind, ScenePrefabDocument, ScenePrefabEntityFlags},
    AssetId,
};

use crate::{
    asset_source::AssetArchives,
    model_source::{
        gltf_model_source::is_gltf_model_asset_path,
        native_model_source_lowering::lower_native_model_source,
    },
    source_document::path::AssetPath,
    texture::blue_fang_source_image_decoding::decode_blue_fang_dds_source_to_rgba,
};

/// Encodes the model at `model_path` as a GLB with one scene. A model that is
/// already replaced by glTF is returned as that glTF. Otherwise the native
/// model's parts are placed as its prefab places them, relative to the prefab
/// root, and each material's base texture is embedded as PNG.
/// `material_texture` names the texture asset path bound to a material, given
/// the material's identity and asset path.
pub fn encode_editable_gltf_model(
    archives: &AssetArchives,
    model_path: &str,
    mut material_texture: impl FnMut(AssetId, &str) -> Option<String>,
) -> Result<Vec<u8>> {
    if let Some(replacement) = archives.live_model_replacement(Path::new(model_path)) {
        return Ok(replacement.to_vec());
    }
    if is_gltf_model_asset_path(Path::new(model_path)) {
        return Ok(archives.read_asset_source(Path::new(model_path))?);
    }
    if let Some(replacement) = archives.gltf_model_replacement(Path::new(model_path))? {
        return Ok(replacement.to_vec());
    }
    let source = archives.read_source(Path::new(model_path))?;
    let lowered = lower_native_model_source(model_path, &source, |reference| {
        archives
            .resolve_model_texture_reference(Path::new(model_path), reference)
            .map(|resolved| resolved.to_string_lossy().into_owned())
    })?;
    let glb = gltf::binary::Glb::from_slice(&lowered.glb)?;
    let mut root: Root = serde_json::from_slice(&glb.json)?;
    let mut binary = glb.bin.map(Cow::into_owned).unwrap_or_default();
    place_model_parts_as_one_scene(&mut root, &lowered.scene_prefab_document, model_path);
    // Lowered materials are named by their identity in hexadecimal.
    for (material_index, material) in root.materials.clone().iter().enumerate() {
        let Some(dependency) =
            lowered
                .scene_prefab_document
                .dependencies
                .iter()
                .find(|dependency| {
                    dependency.asset_kind == ScenePrefabAssetDependencyKind::Material
                        && material.name.as_deref()
                            == Some(
                                dependency
                                    .asset_id
                                    .to_lowercase_hexadecimal_string()
                                    .as_str(),
                            )
                })
        else {
            continue;
        };
        let Some(texture_path) = material_texture(dependency.asset_id, &dependency.asset_path)
        else {
            continue;
        };
        let png = encode_texture_as_png(archives, &texture_path)
            .with_context(|| format!("texture {texture_path} could not be converted"))?;
        let texture = embed_png_texture(&mut root, &mut binary, &png, &texture_path);
        root.materials[material_index]
            .pbr_metallic_roughness
            .base_color_texture = Some(Info {
            index: texture,
            tex_coord: 0,
            extensions: None,
            extras: None,
        });
    }
    if let Some(buffer) = root.buffers.first_mut() {
        buffer.byte_length = USize64(binary.len() as u64);
    }
    gltf::Document::from_json(root.clone())?;
    Ok(gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(serde_json::to_vec(&root)?),
        bin: Some(Cow::Owned(binary)),
    }
    .to_vec()?)
}

/// Wraps a GLB's scene in one node named `openzt2_placement` that places the
/// model where it sits within an object. An editor clears that node's
/// transform before sending the model back.
pub fn place_gltf_scene(
    glb: &[u8],
    translation: [f32; 3],
    rotation_xyzw: [f32; 4],
    scale: [f32; 3],
) -> Result<Vec<u8>> {
    let glb = gltf::binary::Glb::from_slice(glb)?;
    let mut root: Root = serde_json::from_slice(&glb.json)?;
    let scene = root.scene.map_or(0, |scene| scene.value());
    let children = root
        .scenes
        .get(scene)
        .map(|scene| scene.nodes.clone())
        .unwrap_or_default();
    let placement = root.push(Node {
        name: Some("openzt2_placement".to_owned()),
        translation: Some(translation),
        rotation: Some(UnitQuaternion(rotation_xyzw)),
        scale: Some(scale),
        children: (!children.is_empty()).then_some(children),
        ..Default::default()
    });
    if let Some(scene) = root.scenes.get_mut(scene) {
        scene.nodes = vec![placement];
    }
    gltf::Document::from_json(root.clone())?;
    Ok(gltf::binary::Glb {
        header: glb.header,
        json: Cow::Owned(serde_json::to_vec(&root)?),
        bin: glb.bin,
    }
    .to_vec()?)
}

/// The lowered native model keeps each part in its own scene at the origin;
/// the prefab places them. This builds one scene of the visible, most
/// detailed parts in place.
fn place_model_parts_as_one_scene(
    root: &mut Root,
    document: &ScenePrefabDocument,
    model_path: &str,
) {
    let model = AssetId::from_virtual_path(&AssetPath::new(model_path).key());
    let mut placed_nodes = BTreeSet::new();
    let entity_nodes = document
        .entities
        .iter()
        .map(|entity| {
            root.push(Node {
                translation: Some(entity.transform.translation_m),
                rotation: Some(UnitQuaternion(entity.transform.rotation_xyzw)),
                scale: Some(entity.transform.scale),
                ..Default::default()
            })
        })
        .collect::<Vec<_>>();
    for (entity_index, entity) in document.entities.iter().enumerate() {
        let node = entity_nodes[entity_index];
        let shown = entity.flags.0 & ScenePrefabEntityFlags::VISIBLE.0 != 0
            && entity.lods.iter().all(|lod| lod.ordinal == 0);
        let mut children = Vec::new();
        if shown {
            for renderable in entity.renderables.iter().filter(|row| row.model == model) {
                let scene = if renderable.scene_name.is_empty() {
                    root.scene
                        .or_else(|| (!root.scenes.is_empty()).then(|| Index::new(0)))
                } else {
                    root.scenes
                        .iter()
                        .position(|scene| scene.name.as_deref() == Some(&renderable.scene_name))
                        .map(|index| Index::new(index as u32))
                };
                let Some(scene) = scene else {
                    continue;
                };
                // A glTF node has one parent, so a part used twice is placed once.
                children.extend(
                    root.scenes[scene.value()]
                        .nodes
                        .iter()
                        .copied()
                        .filter(|part| placed_nodes.insert(part.value())),
                );
            }
            children.extend(
                entity
                    .children
                    .iter()
                    .map(|child| entity_nodes[*child as usize]),
            );
        }
        root.nodes[node.value()].children = (!children.is_empty()).then_some(children);
    }
    // The replacement is rendered beneath the prefab root, which already
    // applies the root's own transform.
    if let Some(root_node) = entity_nodes.first() {
        let node = &mut root.nodes[root_node.value()];
        node.translation = None;
        node.rotation = None;
        node.scale = None;
    }
    root.scenes = vec![Scene {
        extensions: None,
        extras: None,
        name: Some("model".to_owned()),
        nodes: entity_nodes.first().copied().into_iter().collect(),
    }];
    root.scene = Some(Index::new(0));
}

fn encode_texture_as_png(archives: &AssetArchives, texture_path: &str) -> Result<Vec<u8>> {
    let source = archives.read_asset_source(Path::new(texture_path))?;
    let extension = Path::new(texture_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let image = if matches!(extension.as_str(), "dds" | "z2dds") {
        decode_blue_fang_dds_source_to_rgba(&source)?
    } else {
        image::load_from_memory(&source)?.into_rgba8()
    };
    let mut png = Vec::new();
    image.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)?;
    Ok(png)
}

fn embed_png_texture(
    root: &mut Root,
    binary: &mut Vec<u8>,
    png: &[u8],
    texture_path: &str,
) -> Index<Texture> {
    while binary.len() % 4 != 0 {
        binary.push(0);
    }
    let byte_offset = binary.len();
    binary.extend_from_slice(png);
    let view = root.push(View {
        buffer: Index::new(0),
        byte_length: USize64(png.len() as u64),
        byte_offset: Some(USize64(byte_offset as u64)),
        byte_stride: None,
        name: None,
        target: None,
        extensions: None,
        extras: None,
    });
    let name = Path::new(texture_path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned());
    let image = root.push(Image {
        buffer_view: Some(view),
        mime_type: Some(MimeType("image/png".to_owned())),
        name: name.clone(),
        uri: None,
        extensions: None,
        extras: None,
    });
    root.push(Texture {
        name,
        sampler: None,
        source: image,
        extensions: None,
        extras: None,
    })
}
