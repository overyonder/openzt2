//! glTF models supplied by mods, either beside an original model of the same
//! name or named directly by an object definition.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bevy::math::{Mat4, Vec3};
use openzt2_game_data::{
    scene_prefab::{
        PrefabCollider, PrefabColliderSource, PrefabEntity, PrefabRenderable, PrefabTransform,
        ScenePrefabAssetDependency, ScenePrefabAssetDependencyKind, ScenePrefabColliderLayerFlags,
        ScenePrefabDocument, ScenePrefabEntityFlags, ScenePrefabRenderableVisibilityFlags,
    },
    AssetId,
};

use crate::source_document::path::AssetPath;

/// File extensions of glTF model sources.
pub const GLTF_MODEL_SOURCE_EXTENSIONS: [&str; 2] = ["glb", "gltf"];

/// Asset-path extensions that send glTF model sources through the model
/// loader, which adds the scene prefab that Bevy's plain glTF loader lacks.
/// The archive reader serves `foo.z2glb` from `foo.glb`.
pub const GLTF_MODEL_ASSET_EXTENSIONS: [&str; 2] = ["z2glb", "z2gltf"];

const NATIVE_MODEL_SOURCE_EXTENSIONS: [&str; 2] = ["nif", "bfb"];

pub fn is_model_source_extension(extension: &str) -> bool {
    NATIVE_MODEL_SOURCE_EXTENSIONS
        .iter()
        .chain(&GLTF_MODEL_SOURCE_EXTENSIONS)
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

pub fn is_gltf_model_source_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            GLTF_MODEL_SOURCE_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

pub fn is_gltf_model_asset_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| GLTF_MODEL_ASSET_EXTENSIONS.contains(&extension))
}

/// `foo.glb` becomes `foo.z2glb`; other paths are unchanged.
pub fn gltf_model_asset_path_for_source_path(path: &Path) -> PathBuf {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("glb") => path.with_extension("z2glb"),
        Some(extension) if extension.eq_ignore_ascii_case("gltf") => path.with_extension("z2gltf"),
        _ => path.to_owned(),
    }
}

/// `foo.z2glb` is read from `foo.glb`; other paths are unchanged.
pub fn gltf_model_source_path_for_asset_path(path: &Path) -> PathBuf {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("z2glb") => path.with_extension("glb"),
        Some("z2gltf") => path.with_extension("gltf"),
        _ => path.to_owned(),
    }
}

/// glTF files that replace a native model: `foo.glb` or `foo.gltf` beside
/// `foo.bfb` or `foo.nif`, in the order they are preferred.
pub fn gltf_model_replacement_source_paths(native_model_path: &Path) -> [PathBuf; 2] {
    GLTF_MODEL_SOURCE_EXTENSIONS.map(|extension| native_model_path.with_extension(extension))
}

/// A model sent from an editor while the game runs has no folder of its own,
/// so it must be a binary glTF that embeds its buffers and images.
pub fn validate_gltf_model_replacement(gltf_bytes: &[u8]) -> Result<()> {
    anyhow::ensure!(
        gltf_bytes.starts_with(b"glTF"),
        "the model is not a binary glTF (.glb) file"
    );
    let gltf = gltf::Gltf::from_slice(gltf_bytes)?;
    anyhow::ensure!(gltf.scenes().next().is_some(), "the model has no scene");
    anyhow::ensure!(
        gltf.buffers()
            .all(|buffer| matches!(buffer.source(), gltf::buffer::Source::Bin))
            && gltf
                .images()
                .all(|image| matches!(image.source(), gltf::image::Source::View { .. })),
        "the model refers to files outside it; export it with everything embedded"
    );
    Ok(())
}

/// One root entity rendering the whole default glTF scene, with a box
/// collider and placement footprint taken from the scene's bounds.
pub fn gltf_model_scene_prefab(model_path: &str, gltf_bytes: &[u8]) -> Result<ScenePrefabDocument> {
    let bounds = gltf_default_scene_bounds(gltf_bytes)
        .with_context(|| format!("glTF model {model_path} is not valid"))?;
    let mut root = prefab_entity_with_whole_scene_renderable(model_path, true);
    let mut automatic_placement_bounds_xz = None;
    if let Some((minimum, maximum)) = bounds {
        let half_extent = (maximum - minimum) * 0.5;
        if half_extent.cmpgt(Vec3::ZERO).all() {
            root.colliders.push(PrefabCollider {
                source: PrefabColliderSource::Box {
                    half_extent_m: half_extent.to_array(),
                },
                layer: ScenePrefabColliderLayerFlags::WORLD,
                mask: ScenePrefabColliderLayerFlags::WORLD
                    | ScenePrefabColliderLayerFlags::ANIMAL
                    | ScenePrefabColliderLayerFlags::GUEST
                    | ScenePrefabColliderLayerFlags::STAFF
                    | ScenePrefabColliderLayerFlags::PLACEMENT,
            });
        }
        automatic_placement_bounds_xz = Some([[minimum.x, minimum.z], [maximum.x, maximum.z]]);
        // Colliders sit at their entity's origin, so the collider gets its own
        // entity centred on the bounds.
        if !root.colliders.is_empty() {
            let colliders = std::mem::take(&mut root.colliders);
            let mut collider_entity = empty_prefab_entity("gltf_bounds", 0);
            collider_entity.transform.translation_m = ((minimum + maximum) * 0.5).to_array();
            collider_entity.colliders = colliders;
            root.children.push(1);
            return Ok(ScenePrefabDocument {
                entities: vec![root, collider_entity],
                automatic_placement_bounds_xz,
                rail_camera: None,
                dependencies: vec![model_dependency(model_path)],
            });
        }
    }
    Ok(ScenePrefabDocument {
        entities: vec![root],
        automatic_placement_bounds_xz,
        rail_camera: None,
        dependencies: vec![model_dependency(model_path)],
    })
}

/// Keeps a native model's colliders, effects, attachment points, footprint and
/// placement bounds, and renders its glTF replacement in place of its parts.
/// The replacement's scene is authored relative to the prefab root.
pub fn replace_scene_prefab_renderables_with_whole_gltf_scene(
    mut document: ScenePrefabDocument,
    model_path: &str,
) -> ScenePrefabDocument {
    for entity in &mut document.entities {
        entity.renderables.clear();
        entity.lods.clear();
        entity.local_render_bound = None;
        entity.skin_render_bound = None;
    }
    document
        .dependencies
        .retain(|dependency| dependency.asset_kind != ScenePrefabAssetDependencyKind::Material);
    // Native objects attach their own blob or projected shadow.
    let renderable = prefab_entity_with_whole_scene_renderable(model_path, false)
        .renderables
        .remove(0);
    if let Some(root) = document.entities.first_mut() {
        root.renderables.push(renderable);
    }
    if !document.dependencies.iter().any(|dependency| {
        dependency.asset_kind == ScenePrefabAssetDependencyKind::Model
            && dependency.asset_path == model_path
    }) {
        document.dependencies.push(model_dependency(model_path));
    }
    document
}

fn prefab_entity_with_whole_scene_renderable(model_path: &str, casts_shadow: bool) -> PrefabEntity {
    let mut entity = empty_prefab_entity("gltf_root", u32::MAX);
    let mut visibility = ScenePrefabRenderableVisibilityFlags::VISIBLE
        | ScenePrefabRenderableVisibilityFlags::RECEIVE_SHADOW
        | ScenePrefabRenderableVisibilityFlags::REFLECTION_VISIBLE;
    if casts_shadow {
        visibility = visibility | ScenePrefabRenderableVisibilityFlags::CAST_SHADOW;
    }
    entity.renderables.push(PrefabRenderable {
        model: model_asset_id(model_path),
        scene_name: String::new(),
        material_overrides: Vec::new(),
        visibility,
        material_name: String::new(),
    });
    entity
}

fn empty_prefab_entity(key: &str, parent: u32) -> PrefabEntity {
    PrefabEntity {
        stable_id: AssetId::from_key(key),
        attachment_id: AssetId::from_key(key),
        model_joint_binding: None,
        local_render_bound: None,
        skin_render_bound: None,
        parent,
        transform: PrefabTransform {
            translation_m: [0.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        },
        children: Vec::new(),
        flags: ScenePrefabEntityFlags::VISIBLE | ScenePrefabEntityFlags::ACTIVE,
        rotation_cycles: Vec::new(),
        transform_animations: Vec::new(),
        visibility_animation: None,
        morph_animation: None,
        render_lod_policy: None,
        renderables: Vec::new(),
        colliders: Vec::new(),
        lods: Vec::new(),
        billboards: Vec::new(),
        lights: Vec::new(),
        effects: Vec::new(),
    }
}

fn model_asset_id(model_path: &str) -> AssetId {
    AssetId::from_virtual_path(&AssetPath::new(model_path).key())
}

fn model_dependency(model_path: &str) -> ScenePrefabAssetDependency {
    ScenePrefabAssetDependency {
        asset_id: model_asset_id(model_path),
        asset_path: model_path.to_owned(),
        asset_kind: ScenePrefabAssetDependencyKind::Model,
    }
}

/// Bounds of every mesh in the default scene, from the minimum and maximum
/// that glTF requires on each `POSITION` accessor.
fn gltf_default_scene_bounds(gltf_bytes: &[u8]) -> Result<Option<(Vec3, Vec3)>> {
    let gltf = gltf::Gltf::from_slice(gltf_bytes)?;
    let Some(scene) = gltf.default_scene().or_else(|| gltf.scenes().next()) else {
        return Ok(None);
    };
    let mut bounds: Option<(Vec3, Vec3)> = None;
    let mut pending = scene
        .nodes()
        .map(|node| (node, Mat4::IDENTITY))
        .collect::<Vec<_>>();
    while let Some((node, parent_transform)) = pending.pop() {
        let transform = parent_transform * Mat4::from_cols_array_2d(&node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                let local = primitive.bounding_box();
                let (minimum, maximum) = (Vec3::from(local.min), Vec3::from(local.max));
                for corner in 0..8 {
                    let point = transform.transform_point3(Vec3::new(
                        if corner & 1 == 0 {
                            minimum.x
                        } else {
                            maximum.x
                        },
                        if corner & 2 == 0 {
                            minimum.y
                        } else {
                            maximum.y
                        },
                        if corner & 4 == 0 {
                            minimum.z
                        } else {
                            maximum.z
                        },
                    ));
                    bounds = Some(bounds.map_or((point, point), |(low, high)| {
                        (low.min(point), high.max(point))
                    }));
                }
            }
        }
        pending.extend(node.children().map(|child| (child, transform)));
    }
    Ok(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_glb(translation: [f32; 3]) -> Vec<u8> {
        let positions: [[f32; 3]; 3] = [[-1.0, 0.0, -2.0], [1.0, 0.0, -2.0], [0.0, 3.0, 2.0]];
        let bytes = positions
            .iter()
            .flatten()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        let json = serde_json::json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "nodes": [0] }],
            "nodes": [{ "mesh": 0, "translation": translation }],
            "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 } }] }],
            "accessors": [{
                "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                "min": [-1.0, 0.0, -2.0], "max": [1.0, 3.0, 2.0]
            }],
            "bufferViews": [{ "buffer": 0, "byteLength": bytes.len() }],
            "buffers": [{ "byteLength": bytes.len() }]
        });
        gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: serde_json::to_vec(&json).unwrap().into(),
            bin: Some(bytes.into()),
        }
        .to_vec()
        .unwrap()
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "the bounds are exact sums of the authored positions"
    )]
    fn standalone_gltf_model_is_rendered_whole_and_collides_with_its_bounds() {
        let document =
            gltf_model_scene_prefab("mods/hut.z2glb", &triangle_glb([10.0, 0.0, 0.0])).unwrap();
        assert_eq!(document.entities[0].renderables[0].scene_name, "");
        assert_eq!(
            document.automatic_placement_bounds_xz,
            Some([[9.0, -2.0], [11.0, 2.0]])
        );
        assert_eq!(
            document.entities[1].transform.translation_m,
            [10.0, 1.5, 0.0]
        );
        assert_eq!(
            document.entities[1].colliders[0].source,
            PrefabColliderSource::Box {
                half_extent_m: [1.0, 1.5, 2.0]
            }
        );
        assert_eq!(document.dependencies[0].asset_path, "mods/hut.z2glb");
    }

    #[test]
    fn replacement_keeps_native_colliders_and_renders_only_the_gltf_scene() {
        let mut native = gltf_model_scene_prefab("hut.z2glb", &triangle_glb([0.0; 3])).unwrap();
        native.entities[1].renderables = native.entities[0].renderables.clone();
        native.dependencies.push(ScenePrefabAssetDependency {
            asset_id: AssetId::from_key("material"),
            asset_path: "materials/hut.bfmat".to_owned(),
            asset_kind: ScenePrefabAssetDependencyKind::Material,
        });
        let document = replace_scene_prefab_renderables_with_whole_gltf_scene(native, "hut.bfb");
        assert_eq!(document.entities[0].renderables.len(), 1);
        assert!(document.entities[1].renderables.is_empty());
        assert_eq!(document.entities[1].colliders.len(), 1);
        assert!(document
            .dependencies
            .iter()
            .all(|dependency| dependency.asset_kind == ScenePrefabAssetDependencyKind::Model));
        assert!(document
            .dependencies
            .iter()
            .any(|dependency| dependency.asset_path == "hut.bfb"));
    }
}
