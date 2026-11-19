//! NetImmerse NIF graph, controller, light, effect, and LOD scene lowering.

use std::collections::BTreeSet;

use openzt2_game_data::{
    scene_prefab::{
        PrefabBillboard, PrefabEffect, PrefabLight, PrefabLightKind, PrefabRotationCycle,
        PrefabTransform, ScenePrefabAssetDependencyKind, ScenePrefabDocument,
    },
    AssetId,
};

use crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

use super::super::{
    native_geometry::{
        native_geometry_identity::NativeMaterialReference,
        native_geometry_identity::{
            netimmerse_geometry_skin_variant_path, netimmerse_geometry_virtual_path,
        },
        nif_material_lowering::inherited_netimmerse_property_references,
        source_transform_conversion::convert_source_transform_to_bevy_components,
    },
    native_model_source_lowering::{
        native_model_material_labelled_asset_path, native_model_particle_effect_labelled_asset_path,
    },
    netimmerse_bone_level_of_detail_selection::NetImmerseBoneLod,
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
    },
};
use super::{
    native_scene_lowering_error::NativeSceneLoweringError,
    nif_collider_lowering::lower_netimmerse_collision_volume,
    nif_object_transform_controller_lowering::{
        lower_netimmerse_object_transform_motion, NetImmerseObjectTransformMotion,
    },
    scene_prefab_document_assembly::ScenePrefabDocumentAssembler,
    scene_transform_and_collider_math::netimmerse_point_light_range,
};

type Result<T> = std::result::Result<T, NativeSceneLoweringError>;

pub(in super::super) fn lower_netimmerse_scene_prefab(
    document: &NetImmerseNifDocument,
    bone_lod: &NetImmerseBoneLod,
    source: &str,
    animation_manifest: Option<&str>,
    footprint_collision_hull: Option<Vec<[f32; 3]>>,
) -> Result<ScenePrefabDocument> {
    let model_path = source.to_owned();
    let mut builder = ScenePrefabDocumentAssembler::new(&model_path);
    let mut visiting = BTreeSet::new();
    for root in &document.footer.root_block_references {
        visit_netimmerse_scene_block(document, bone_lod, *root, None, &mut visiting, &mut builder)?;
    }
    if builder.is_empty() {
        for block in document.blocks() {
            if block.payload.av_object().is_some() {
                visit_netimmerse_scene_block(
                    document,
                    bone_lod,
                    block.index as i32,
                    None,
                    &mut visiting,
                    &mut builder,
                )?;
            }
        }
    }
    if let Some(path) = animation_manifest {
        builder.dependency(
            AssetId::from_virtual_path(path),
            path.to_owned(),
            ScenePrefabAssetDependencyKind::Animation,
        );
    }
    builder.replace_colliders_with_footprint_collision_hull(footprint_collision_hull)?;
    builder.finish()
}

fn visit_netimmerse_scene_block(
    document: &NetImmerseNifDocument,
    bone_lod: &NetImmerseBoneLod,
    block_ref: i32,
    parent: Option<u32>,
    visiting: &mut BTreeSet<i32>,
    builder: &mut ScenePrefabDocumentAssembler,
) -> Result<()> {
    if block_ref < 0 || builder.contains_netimmerse_entity(block_ref) {
        return Ok(());
    }
    if !visiting.insert(block_ref) {
        return Err(NativeSceneLoweringError::new(
            &document.source_path,
            format!("contains a cyclic scene graph at block {block_ref}"),
        ));
    }
    let Some(block) = document.block(block_ref) else {
        return Err(NativeSceneLoweringError::new(
            &document.source_path,
            format!("references missing NIF scene block {block_ref}"),
        ));
    };
    let Some(object) = block.payload.av_object() else {
        visiting.remove(&block_ref);
        return Ok(());
    };
    let (mut translation_m, mut rotation_xyzw, mut scale) =
        convert_source_transform_to_bevy_components(
            object.translation,
            object.rotation,
            object.scale,
        );
    let object_controller_is_owned_by_particle_simulation = matches!(
        &block.payload,
        NetImmerseNifBlockPayload::NiParticles(_) | NetImmerseNifBlockPayload::NiParticleMeshes(_)
    );
    let transform_motion = if object_controller_is_owned_by_particle_simulation {
        NetImmerseObjectTransformMotion::None
    } else {
        lower_netimmerse_object_transform_motion(document, object)?
    };
    if let NetImmerseObjectTransformMotion::ConstantPose {
        translation_m: constant_translation_m,
        rotation_xyzw: constant_rotation_xyzw,
        uniform_scale: constant_uniform_scale,
    } = &transform_motion
    {
        translation_m = constant_translation_m.unwrap_or(translation_m);
        rotation_xyzw = constant_rotation_xyzw.unwrap_or(rotation_xyzw);
        scale = constant_uniform_scale.map_or(scale, |value| [value; 3]);
    }
    let node_lod = bone_lod.nodes.get(&block_ref);
    let shape_lod = bone_lod.shapes.get(&block_ref);
    let visible = node_lod.map_or_else(
        || shape_lod.is_some() || object.flags & 1 == 0,
        |node| node.initial == node.ordinal,
    );
    let parent_key = parent.map(|parent| builder.entity_key(parent).to_owned());
    let entity = builder.entity_with_distinct_stable_and_authored_attachment_keys(
        &netimmerse_scene_entity_key(block_ref),
        &object.object.name,
        parent_key.as_deref(),
        PrefabTransform {
            translation_m,
            rotation_xyzw,
            scale,
        },
        visible,
    )?;
    builder.remember_netimmerse_entity(block_ref, entity);
    let geometry = match &block.payload {
        NetImmerseNifBlockPayload::NiTriShape(shape) => Some(&shape.geometry),
        NetImmerseNifBlockPayload::NiTriStrips(strips) => Some(&strips.geometry),
        _ => None,
    };
    let local_bound = geometry
        .and_then(|geometry| document.block(geometry.data_ref))
        .and_then(|data| match &data.payload {
            NetImmerseNifBlockPayload::NiTriShapeData(data) => Some(&data.geometry),
            NetImmerseNifBlockPayload::NiTriStripsData(data) => Some(&data.geometry),
            _ => None,
        })
        .map(|bound| {
            (
                convert_source_z_up_vector_to_bevy_y_up_coordinates(bound.center),
                bound.radius,
            )
        });
    if let Some((centre, radius)) = local_bound {
        builder.local_render_bound(entity, centre, radius);
    }
    if shape_lod.is_none() {
        if let Some(geometry) = geometry {
            if let Some(bound) = netimmerse_skin_render_bound(document, geometry.skin_instance_ref)?
            {
                builder.skin_render_bound(entity, bound);
            }
        }
    }
    if let Some(node) = node_lod {
        let group = bone_lod_controller_row(builder, node.controller, entity)?;
        add_bone_lod_membership(builder, entity, group, node.ordinal, node.initial);
    }
    if geometry.is_some() {
        if let Some(shape) = shape_lod {
            let parent_key = builder.entity_key(entity).to_owned();
            let group = bone_lod_controller_row(builder, shape.controller, entity)?;
            for (&ordinal, &skin) in &shape.skins {
                let variant = builder.entity(
                    &format!("{parent_key}_bone_lod_{ordinal}"),
                    Some(&parent_key),
                    PrefabTransform {
                        translation_m: [0.0; 3],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                        scale: [1.0; 3],
                    },
                    ordinal == shape.initial,
                )?;
                let path = if ordinal == shape.initial {
                    netimmerse_geometry_virtual_path(document, block.index)
                } else {
                    netimmerse_geometry_skin_variant_path(document, block.index, ordinal)
                };
                add_netimmerse_renderable(
                    builder,
                    document,
                    &block.payload,
                    block.index,
                    variant,
                    path,
                );
                if let Some((centre, radius)) = local_bound {
                    builder.local_render_bound(variant, centre, radius);
                }
                if let Some(bound) = netimmerse_skin_render_bound(document, skin)? {
                    builder.skin_render_bound(variant, bound);
                }
                add_bone_lod_membership(builder, variant, group, ordinal, shape.initial);
            }
        } else {
            add_netimmerse_renderable(
                builder,
                document,
                &block.payload,
                block.index,
                entity,
                netimmerse_geometry_virtual_path(document, block.index),
            );
        }
    }
    match &block.payload {
        NetImmerseNifBlockPayload::NiBillboardNode(node) => builder.billboard(
            entity,
            PrefabBillboard {
                mode: node.billboard_mode,
                faces_positive_z: false,
            },
        ),
        NetImmerseNifBlockPayload::NiLODNode(node) => {
            let center_m = node
                .lod_center
                .map(convert_source_z_up_vector_to_bevy_y_up_coordinates)
                .unwrap_or([0.0; 3]);
            for (ordinal, child) in node.switch_node.node.child_refs.iter().enumerate() {
                if *child < 0 {
                    continue;
                }
                let range = node.lod_levels.get(ordinal);
                builder.pending_netimmerse_lod(
                    *child,
                    entity,
                    u16::try_from(ordinal).map_err(|_| {
                        NativeSceneLoweringError::new(
                            &document.source_path,
                            format!("NIF LOD ordinal {ordinal} exceeds u16::MAX"),
                        )
                    })?,
                    center_m,
                    range.map_or(0.0, |range| range.near_extent),
                    range.map_or(0.0, |range| range.far_extent),
                    range.is_none() && node.switch_node.active_child_index as usize == ordinal,
                );
            }
        }
        NetImmerseNifBlockPayload::NiPointLight(light) => builder.light(
            entity,
            PrefabLight {
                kind: PrefabLightKind::Point,
                color_srgb: light.light.diffuse,
                intensity: 800.0 * light.light.dimmer.max(0.0),
                range_m: netimmerse_point_light_range(
                    light.constant_attenuation,
                    light.linear_attenuation,
                    light.quadratic_attenuation,
                ),
            },
        ),
        NetImmerseNifBlockPayload::NiAmbientLight(light) => builder.light(
            entity,
            PrefabLight {
                kind: PrefabLightKind::Ambient,
                color_srgb: light.light.ambient,
                intensity: light.light.dimmer.max(0.0),
                range_m: 0.0,
            },
        ),
        NetImmerseNifBlockPayload::NiDirectionalLight(light) => builder.light(
            entity,
            PrefabLight {
                kind: PrefabLightKind::Directional,
                color_srgb: light.light.diffuse,
                intensity: light.light.dimmer.max(0.0),
                range_m: 0.0,
            },
        ),
        NetImmerseNifBlockPayload::NiParticles(_)
        | NetImmerseNifBlockPayload::NiParticleMeshes(_) => {
            let effect = AssetId::from_key(&format!(
                "{}.__effect/nif_{:08x}",
                document.source_path.as_str(),
                block.index
            ));
            builder.effect(entity, PrefabEffect { effect });
            builder.dependency(
                effect,
                native_model_particle_effect_labelled_asset_path(
                    document.source_path.as_str(),
                    block.index,
                ),
                ScenePrefabAssetDependencyKind::Effect,
            );
        }
        _ => {}
    }
    match transform_motion {
        NetImmerseObjectTransformMotion::RotationCycle(speed) => builder.rotation_cycle(
            entity,
            PrefabRotationCycle {
                radians_per_second: speed,
            },
        ),
        NetImmerseObjectTransformMotion::Animation(animation) => {
            builder.transform_animation(entity, *animation);
        }
        NetImmerseObjectTransformMotion::None
        | NetImmerseObjectTransformMotion::ConstantPose { .. } => {}
    }
    if object.collision_object_ref >= 0 {
        if let Some(NetImmerseNifBlockPayload::NiCollisionData(collision)) = document
            .block(object.collision_object_ref)
            .map(|block| &block.payload)
        {
            if let Some(volume) = collision.bounding_volume.as_ref() {
                lower_netimmerse_collision_volume(
                    builder,
                    entity,
                    volume,
                    document.source_path.as_str(),
                )?;
            }
        }
    }
    let (children, effects) = block.payload.child_and_effect_refs();
    for child in children.iter().chain(effects) {
        visit_netimmerse_scene_block(document, bone_lod, *child, Some(entity), visiting, builder)?;
    }
    visiting.remove(&block_ref);
    Ok(())
}

fn netimmerse_scene_entity_key(block: i32) -> String {
    format!("nif_{:08x}", block as u32)
}

/// Name of the `NiMaterialProperty` a geometry block renders with, including
/// properties inherited from its ancestors. Material lowering reports
/// malformed inheritance, so a failure here only leaves the name unset.
fn netimmerse_geometry_material_name<'a>(
    document: &'a NetImmerseNifDocument,
    geometry_block: u32,
    payload: &NetImmerseNifBlockPayload,
) -> Option<&'a str> {
    let geometry = match payload {
        NetImmerseNifBlockPayload::NiTriShape(value) => &value.geometry,
        NetImmerseNifBlockPayload::NiTriStrips(value) => &value.geometry,
        _ => return None,
    };
    inherited_netimmerse_property_references(document, geometry_block, geometry)
        .ok()?
        .into_iter()
        .filter_map(|reference| document.block(reference))
        .find_map(|block| match &block.payload {
            NetImmerseNifBlockPayload::NiMaterialProperty(material) => {
                Some(material.object.name.as_str())
            }
            _ => None,
        })
        .filter(|name| !name.is_empty())
}

fn netimmerse_skin_render_bound(
    document: &NetImmerseNifDocument,
    skin_index: i32,
) -> Result<Option<openzt2_game_data::scene_prefab::PrefabSkinRenderBound>> {
    if let Some(instance) = document
        .block(skin_index)
        .and_then(|block| match &block.payload {
            NetImmerseNifBlockPayload::NiSkinInstance(instance) => Some(instance),
            _ => None,
        })
    {
        let data = document
            .block(instance.data_ref)
            .and_then(|block| match &block.payload {
                NetImmerseNifBlockPayload::NiSkinData(data) => Some(data),
                _ => None,
            })
            .ok_or_else(|| {
                NativeSceneLoweringError::new(
                    &document.source_path,
                    "skin bound references missing skin data",
                )
            })?;
        if data.bones.len() != instance.bone_refs.len() {
            return Err(NativeSceneLoweringError::new(
                &document.source_path,
                "skin bound bone counts differ",
            ));
        }
        let joint_name = |reference| {
            document
                .block(reference)
                .and_then(|block| block.payload.av_object())
                .map(|object| object.object.name.clone())
                .ok_or_else(|| {
                    NativeSceneLoweringError::new(
                        &document.source_path,
                        "skin bound references a missing joint",
                    )
                })
        };
        Ok(Some(openzt2_game_data::scene_prefab::PrefabSkinRenderBound {
            skeleton_root: joint_name(instance.skeleton_root_ref)?,
            skin_transform: super::super::native_geometry::source_transform_conversion::convert_netimmerse_transform_to_gltf_matrix(&data.skin_transform),
            bones: instance.bone_refs.iter().zip(&data.bones).map(|(reference, bone)| {
                Ok(openzt2_game_data::scene_prefab::PrefabSkinBoneRenderBound {
                    joint: joint_name(*reference)?,
                    sphere: openzt2_game_data::scene_prefab::PrefabBoundingSphere {
                        center: convert_source_z_up_vector_to_bevy_y_up_coordinates(bone.bounding_sphere.center),
                        radius: bone.bounding_sphere.radius,
                    },
                })
            }).collect::<Result<Vec<_>>>()?,
        }))
    } else {
        Ok(None)
    }
}

fn bone_lod_controller_row(
    builder: &mut ScenePrefabDocumentAssembler,
    controller: u32,
    anchor: u32,
) -> Result<u32> {
    let key = format!("nif_bone_lod_controller_{controller}");
    if let Some(entity) = builder.entity_index(&key) {
        return Ok(entity);
    }
    let parent = builder.entity_key(anchor).to_owned();
    builder.entity(
        &key,
        Some(&parent),
        PrefabTransform {
            translation_m: [0.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        },
        true,
    )
}

fn add_bone_lod_membership(
    builder: &mut ScenePrefabDocumentAssembler,
    entity: u32,
    group: u32,
    ordinal: u16,
    initial: u16,
) {
    builder.lod(
        entity,
        openzt2_game_data::scene_prefab::PrefabLod {
            group_entity: group,
            ordinal,
            center_m: [0.0; 3],
            near_m: 0.0,
            far_m: 0.0,
            active_without_range: ordinal == initial,
            initial_render_tier: Some(initial),
        },
    );
}

fn add_netimmerse_renderable(
    builder: &mut ScenePrefabDocumentAssembler,
    document: &NetImmerseNifDocument,
    payload: &NetImmerseNifBlockPayload,
    block: u32,
    entity: u32,
    scene_path: String,
) {
    builder.renderable(entity, scene_path);
    let material = AssetId::from_key(
        &NativeMaterialReference::NetImmerseGeometry {
            document: &document.source_path,
            block,
        }
        .asset_key(),
    );
    builder.material_override_from(
        material,
        native_model_material_labelled_asset_path(&document.source_path, block),
    );
    if let Some(name) = netimmerse_geometry_material_name(document, block, payload) {
        builder.renderable_material_name(name);
    }
}
