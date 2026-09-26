//! Blue Fang BFB hierarchy, renderable, LOD, and collider scene lowering.

use openzt2_game_data::{
    scene_prefab::{
        PrefabBillboard, PrefabColliderSource, PrefabEffect, PrefabLod, PrefabTransform,
        ScenePrefabAssetDependencyKind, ScenePrefabDocument,
    },
    AssetId,
};

use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

use super::super::{
    blue_fang_bfb_source::{document::BlueFangBfbDocument, source_types::BlueFangBfbNodeKind},
    native_geometry::native_geometry_identity::blue_fang_geometry_virtual_path,
};
use super::{
    native_scene_lowering_error::NativeSceneLoweringError,
    scene_prefab_document_assembly::ScenePrefabDocumentAssembler,
    scene_transform_and_collider_math::{
        convert_blue_fang_scene_matrix_to_prefab_transform, identity_prefab_transform,
        rotation_from_source_positive_y_axis, source_vector_length, translated_prefab_transform,
    },
};

type Result<T> = std::result::Result<T, NativeSceneLoweringError>;

fn bfb_key(index: usize) -> String {
    format!("bfb_{index:08x}")
}

pub(in super::super) fn lower_blue_fang_scene_prefab(
    document: &BlueFangBfbDocument,
    source: &str,
    animation_manifest: Option<&str>,
) -> Result<ScenePrefabDocument> {
    let model_path = source.to_owned();
    let mut builder = ScenePrefabDocumentAssembler::new(&model_path);
    for (index, node) in document.hierarchy().enumerate() {
        let key = bfb_key(index);
        let parent = node.parent.map(bfb_key);
        let transform = convert_blue_fang_scene_matrix_to_prefab_transform(node.local_transform)
            .unwrap_or_else(identity_prefab_transform);
        let entity = builder.entity_with_distinct_stable_and_authored_attachment_keys(
            &key,
            &node.name,
            parent.as_deref(),
            transform,
            true,
        )?;
        match &node.kind {
            BlueFangBfbNodeKind::MeshLink { material_name, .. }
            | BlueFangBfbNodeKind::Billboard { material_name, .. } => {
                let scene = blue_fang_geometry_virtual_path(document, index);
                builder.renderable_from(entity, &model_path, scene);
                builder.blue_fang_model_material_override(material_name);
            }
            BlueFangBfbNodeKind::Node | BlueFangBfbNodeKind::LodGroup => {}
            BlueFangBfbNodeKind::ModelJoint { component_name } => {
                let joint = document
                    .mesh_blocks()
                    .filter_map(|block| block.skin.as_ref())
                    .flat_map(|skin| &skin.bones)
                    .find(|bone| {
                        !component_name.is_empty() && bone.name.eq_ignore_ascii_case(component_name)
                    });
                let Some(joint) = joint else {
                    return Err(NativeSceneLoweringError::new(
                        &document.source_path,
                        format!("BFB attachment references missing model joint {component_name:?}"),
                    ));
                };
                builder.bind_model_joint(entity, &joint.name);
            }
            BlueFangBfbNodeKind::ParticleSystem { resource_name } => {
                // An empty authored reference leaves the attachment node empty
                // in the native loader, while retaining its children/transform.
                if !resource_name.is_empty() {
                    let resource_path = if resource_name.to_ascii_lowercase().ends_with(".psys") {
                        format!("particle/{resource_name}")
                    } else {
                        format!("particle/{resource_name}.psys")
                    };
                    let effect = AssetId::from_virtual_path(&resource_path);
                    builder.effect(entity, PrefabEffect { effect });
                    builder.dependency(
                        effect,
                        resource_path,
                        ScenePrefabAssetDependencyKind::Effect,
                    );
                }
            }
            BlueFangBfbNodeKind::Other(kind) => {
                return Err(NativeSceneLoweringError::new(
                    &document.source_path,
                    format!("contains unsupported BFB hierarchy node kind {kind}"),
                ));
            }
        }
        if let BlueFangBfbNodeKind::Billboard { mode, .. } = &node.kind {
            builder.billboard(entity, PrefabBillboard { mode: *mode });
        }
        if let Some(ordinal) = node.lod_index {
            builder.lod(
                entity,
                PrefabLod {
                    group_entity: node
                        .parent
                        .and_then(|parent| builder.entity_index(&bfb_key(parent)))
                        .unwrap_or(entity),
                    ordinal: u16::try_from(ordinal).map_err(|_| {
                        NativeSceneLoweringError::new(
                            &document.source_path,
                            format!("BFB LOD ordinal {ordinal} exceeds u16::MAX"),
                        )
                    })?,
                    center_m: [0.0; 3],
                    near_m: 0.0,
                    far_m: 0.0,
                    active_without_range: ordinal == 0,
                },
            );
        }
    }
    for collision in document.collision_boxes() {
        if collision
            .half_extents
            .iter()
            .all(|extent| extent.is_finite() && *extent > 0.0)
        {
            let entity = builder.entity(
                &format!("bfb_box_{:08x}", collision.block_id),
                None,
                convert_blue_fang_scene_matrix_to_prefab_transform(collision.transform)
                    .unwrap_or_else(identity_prefab_transform),
                true,
            )?;
            builder.collider(
                entity,
                PrefabColliderSource::Box {
                    half_extent_m: [
                        collision.half_extents[0],
                        collision.half_extents[2],
                        collision.half_extents[1],
                    ],
                },
            );
        }
    }
    for collision in document.collision_spheres() {
        if collision.radius.is_finite() && collision.radius > 0.0 {
            let entity = builder.entity(
                &format!("bfb_sphere_{:08x}", collision.block_id),
                None,
                translated_prefab_transform(convert_source_z_up_vector_to_bevy_y_up_coordinates(
                    collision.center,
                )),
                true,
            )?;
            builder.collider(
                entity,
                PrefabColliderSource::Capsule {
                    radius_m: collision.radius,
                    half_height_m: 0.0,
                },
            );
        }
    }
    for collision in document.collision_capsules() {
        let start = convert_source_z_up_vector_to_bevy_y_up_coordinates(collision.start);
        let end = convert_source_z_up_vector_to_bevy_y_up_coordinates(collision.end);
        let axis = std::array::from_fn(|index| end[index] - start[index]);
        let length = source_vector_length(axis);
        if collision.radius.is_finite() && collision.radius > 0.0 && length > f32::EPSILON {
            let entity = builder.entity(
                &format!("bfb_capsule_{:08x}", collision.block_id),
                None,
                PrefabTransform {
                    translation_m: std::array::from_fn(|index| (start[index] + end[index]) * 0.5),
                    rotation_xyzw: rotation_from_source_positive_y_axis(
                        axis.map(|value| value / length),
                    ),
                    scale: [1.0; 3],
                },
                true,
            )?;
            builder.collider(
                entity,
                PrefabColliderSource::Capsule {
                    radius_m: collision.radius,
                    half_height_m: length * 0.5,
                },
            );
        }
    }
    if let Some(path) = animation_manifest {
        builder.dependency(
            AssetId::from_virtual_path(path),
            path.to_owned(),
            ScenePrefabAssetDependencyKind::Animation,
        );
    }
    builder.finish()
}
