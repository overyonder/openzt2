//! Blue Fang BFB hierarchy, renderable, LOD, and collider scene lowering.

use std::collections::{BTreeMap, BTreeSet};

use openzt2_game_data::{
    scene_prefab::{
        PrefabBillboard, PrefabColliderSource, PrefabEffect, PrefabLod, PrefabRenderLodPolicy,
        PrefabRenderLodSource, PrefabTransform, ScenePrefabAssetDependencyKind,
        ScenePrefabDocument,
    },
    AssetId,
};

use crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

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

fn particle_resource_path(name: &str) -> Option<String> {
    (!name.is_empty()).then(|| {
        if name.to_ascii_lowercase().ends_with(".psys") {
            format!("particle/{name}")
        } else {
            format!("particle/{name}.psys")
        }
    })
}

pub(in super::super) fn lower_blue_fang_scene_prefab(
    document: &BlueFangBfbDocument,
    source: &str,
) -> Result<ScenePrefabDocument> {
    let model_path = source.to_owned();
    let mut builder = ScenePrefabDocumentAssembler::new(&model_path);
    let mut lod_groups = BTreeMap::<usize, (u32, u16)>::new();
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
            BlueFangBfbNodeKind::MeshLink {
                material_names,
                mesh_block_id,
            }
            | BlueFangBfbNodeKind::Billboard {
                material_names,
                mesh_block_id,
                ..
            } => {
                if let Some(mesh) = document
                    .mesh_blocks()
                    .find(|mesh| mesh.block_id == *mesh_block_id)
                {
                    for (section_index, (section, material_name)) in
                        mesh.sections.iter().zip(material_names).enumerate()
                    {
                        let section_entity = builder.entity(
                            &format!("{key}_section_{section_index:08x}"),
                            Some(&key),
                            identity_prefab_transform(),
                            true,
                        )?;
                        builder.local_render_bound(
                            section_entity,
                            convert_source_z_up_vector_to_bevy_y_up_coordinates(
                                section.bounds_center,
                            ),
                            section.bounds_radius,
                        );
                        let scene = blue_fang_geometry_virtual_path(document, index, section_index);
                        builder.renderable_from(section_entity, &model_path, scene);
                        builder.blue_fang_model_material_override(material_name);
                    }
                }
            }
            BlueFangBfbNodeKind::Node => {}
            BlueFangBfbNodeKind::LodGroup => {
                lod_groups.insert(index, (entity, 0));
            }
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
                if let Some(resource_path) = particle_resource_path(resource_name) {
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
            builder.billboard(
                entity,
                PrefabBillboard {
                    mode: *mode,
                    faces_positive_z: true,
                },
            );
        }
        if let Some((group_entity, child_count)) =
            node.parent.and_then(|parent| lod_groups.get_mut(&parent))
        {
            let ordinal = *child_count;
            *child_count = ordinal.checked_add(1).ok_or_else(|| {
                NativeSceneLoweringError::new(&document.source_path, "too many BFB LOD children")
            })?;
            builder.lod(
                entity,
                PrefabLod {
                    group_entity: *group_entity,
                    ordinal,
                    center_m: [0.0; 3],
                    near_m: 0.0,
                    far_m: 0.0,
                    active_without_range: ordinal == 0,
                    initial_render_tier: Some(0),
                },
            );
        }
    }
    for (entity, tier_count) in lod_groups.into_values() {
        if tier_count == 0 {
            continue;
        }
        builder.render_lod_policy(
            entity,
            PrefabRenderLodPolicy {
                source: PrefabRenderLodSource::BlueFang,
                metric_entity: entity,
                initial_tier: 0,
                tier_count,
                switch_distance_squared: (0..tier_count)
                    .map(|tier| f32::from(tier) * 225.0)
                    .collect(),
                minimum_switch_distance_squared: vec![0.0; usize::from(tier_count)],
            },
        );
    }
    let footprint_shape_blocks = document
        .hierarchy()
        .filter(|node| node.name.eq_ignore_ascii_case("footprint"))
        .filter_map(|node| node.block_id.checked_add(1))
        .collect::<BTreeSet<_>>();
    for collision in document
        .collision_boxes()
        .filter(|collision| !footprint_shape_blocks.contains(&collision.block_id))
    {
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
    builder.finish()
}
