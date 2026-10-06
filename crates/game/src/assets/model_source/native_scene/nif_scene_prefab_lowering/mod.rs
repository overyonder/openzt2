//! NetImmerse NIF graph, controller, light, effect, and LOD scene lowering.

use std::collections::BTreeSet;

use openzt2_game_data::{
    scene_prefab::{
        PrefabBillboard, PrefabEffect, PrefabLight, PrefabLightKind, PrefabRotationCycle,
        PrefabTransform, ScenePrefabAssetDependencyKind, ScenePrefabDocument,
    },
    AssetId,
};

use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

use super::super::{
    native_geometry::{
        native_geometry_identity::netimmerse_geometry_virtual_path,
        native_geometry_identity::NativeMaterialReference,
        nif_material_lowering::inherited_netimmerse_property_references,
        source_transform_conversion::convert_source_transform_to_bevy_components,
    },
    native_model_source_lowering::{
        native_model_material_labelled_asset_path, native_model_particle_effect_labelled_asset_path,
    },
    netimmerse_bone_level_of_detail_selection::NetImmerseSelectedBoneLevelOfDetail,
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
    selected_bone_level_of_detail: &NetImmerseSelectedBoneLevelOfDetail,
    source: &str,
    animation_manifest: Option<&str>,
) -> Result<ScenePrefabDocument> {
    let model_path = source.to_owned();
    let mut builder = ScenePrefabDocumentAssembler::new(&model_path);
    let mut visiting = BTreeSet::new();
    for root in &document.footer.root_block_references {
        visit_netimmerse_scene_block(
            document,
            selected_bone_level_of_detail,
            *root,
            None,
            &mut visiting,
            &mut builder,
        )?;
    }
    if builder.is_empty() {
        for block in document.blocks() {
            if block.payload.av_object().is_some() {
                visit_netimmerse_scene_block(
                    document,
                    selected_bone_level_of_detail,
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
    builder.finish()
}

fn visit_netimmerse_scene_block(
    document: &NetImmerseNifDocument,
    selected_bone_level_of_detail: &NetImmerseSelectedBoneLevelOfDetail,
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
    let parent_key = parent.map(|parent| builder.entity_key(parent).to_owned());
    let entity = builder.entity(
        &netimmerse_scene_entity_key(block_ref),
        parent_key.as_deref(),
        PrefabTransform {
            translation_m,
            rotation_xyzw,
            scale,
        },
        selected_bone_level_of_detail
            .selected_or_authored_scene_block_visibility(block_ref, object.flags & 1 == 0),
    )?;
    builder.remember_netimmerse_entity(block_ref, entity);
    if selected_bone_level_of_detail.scene_block_is_visible(block_ref)
        && matches!(
            &block.payload,
            NetImmerseNifBlockPayload::NiTriShape(_) | NetImmerseNifBlockPayload::NiTriStrips(_)
        )
    {
        builder.renderable(
            entity,
            netimmerse_geometry_virtual_path(document, block.index),
        );
        let material = AssetId::from_key(
            &NativeMaterialReference::NetImmerseGeometry {
                document: &document.source_path,
                block: block.index,
            }
            .asset_key(),
        );
        builder.material_override_from(
            material,
            native_model_material_labelled_asset_path(&document.source_path, block.index),
        );
        if let Some(name) = netimmerse_geometry_material_name(document, block.index, &block.payload)
        {
            builder.renderable_material_name(name);
        }
    }
    match &block.payload {
        NetImmerseNifBlockPayload::NiBillboardNode(node) => builder.billboard(
            entity,
            PrefabBillboard {
                // NiBillboardNode did not serialize its mode before NIF 10.1.
                // Gamebryo constructs those nodes as ROTATE_ABOUT_UP.
                mode: node.billboard_mode.unwrap_or(1),
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
        visit_netimmerse_scene_block(
            document,
            selected_bone_level_of_detail,
            *child,
            Some(entity),
            visiting,
            builder,
        )?;
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
