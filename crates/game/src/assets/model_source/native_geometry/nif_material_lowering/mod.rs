//! NetImmerse inherited-property and native-material lowering.

use std::collections::HashSet;

use super::super::{
    model::{NativeMaterialSource, NativeTextureSource},
    native_geometry_lowering_error::{
        NativeGeometryLoweringError, NativeGeometryLoweringErrorKind, NativeGeometrySourceFamily,
    },
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
        scene_object_source_types::NetImmerseNiGeometry,
    },
};
use super::validated_vertex_topology_assembly::native_geometry_lowering_error;

pub(super) fn lower_netimmerse_native_material(
    document: &NetImmerseNifDocument,
    geometry_block: u32,
    geometry: &NetImmerseNiGeometry,
    resolve_texture: &mut impl FnMut(&str) -> Option<String>,
) -> Result<Option<NativeMaterialSource>, NativeGeometryLoweringError> {
    let property_refs =
        inherited_netimmerse_property_references(document, geometry_block, geometry)?;
    let properties = property_refs
        .iter()
        .map(|reference| {
            document.block(*reference).ok_or_else(|| {
                native_geometry_lowering_error(
                    NativeGeometrySourceFamily::Nif,
                    &document.source_path,
                    NativeGeometryLoweringErrorKind::InvalidMaterial {
                        detail: format!("missing inherited property block {reference}"),
                    },
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let material = properties.iter().find_map(|block| match &block.payload {
        NetImmerseNifBlockPayload::NiMaterialProperty(value) => Some(value),
        _ => None,
    });
    let texturing = properties.iter().find_map(|block| match &block.payload {
        NetImmerseNifBlockPayload::NiTexturingProperty(value) => Some(value),
        _ => None,
    });
    // Missing optional textures leave their stages unbound.
    let mut lower_texture_slot = |slot: Option<&super::super::netimmerse_nif_source::render_property_and_texture_source_types::NetImmerseTextureSlot>| {
        slot.and_then(|slot| {
            document
                .block(slot.source_ref)
                .and_then(|block| match &block.payload {
                    NetImmerseNifBlockPayload::NiSourceTexture(value) => {
                        value.file_name.as_deref()
                    }
                    _ => None,
                })
                .and_then(&mut *resolve_texture)
                .map(|path| NativeTextureSource {
                    asset_path: path.replace('\\', "/"),
                    uv_set: slot.uv_set,
                    clamp_mode: slot.clamp_mode,
                    filter_mode: slot.filter_mode,
                })
        })
    };
    let base_texture = lower_texture_slot(texturing.and_then(|value| value.base_texture.as_ref()));
    let detail_texture =
        lower_texture_slot(texturing.and_then(|value| value.detail_texture.as_ref()));
    let glow_texture = lower_texture_slot(texturing.and_then(|value| value.glow_texture.as_ref()));
    let alpha = properties.iter().find_map(|block| match &block.payload {
        NetImmerseNifBlockPayload::NiAlphaProperty(value) => Some(value),
        _ => None,
    });
    let depth = properties.iter().find_map(|block| match &block.payload {
        NetImmerseNifBlockPayload::NiZBufferProperty(value) => Some(value),
        _ => None,
    });
    // NetImmerse stores alpha blending at bit 0 and alpha testing at bit 9.
    let alpha_blend = alpha.is_some_and(|value| value.flags & 0x0001 != 0);
    let alpha_cutoff = alpha
        .filter(|value| value.flags & 0x0200 != 0)
        .map(|value| f32::from(value.threshold) / 255.0);
    let base_color = material.map_or([1.0; 4], |value| {
        [
            value.diffuse[0],
            value.diffuse[1],
            value.diffuse[2],
            value.alpha,
        ]
    });
    let vertex_color = properties.iter().find_map(|block| match &block.payload {
        NetImmerseNifBlockPayload::NiVertexColorProperty(value) => Some(value),
        _ => None,
    });
    let emissive = material.map_or([0.0; 3], |value| value.emissive);
    let stencil = property_refs
        .iter()
        .filter_map(|reference| document.block(*reference))
        .find_map(|block| match &block.payload {
            NetImmerseNifBlockPayload::NiStencilProperty(value) => Some(value),
            _ => None,
        });
    if stencil.is_some_and(|value| value.stencil_enabled != 0) {
        return Err(native_geometry_lowering_error(
            NativeGeometrySourceFamily::Nif,
            &document.source_path,
            NativeGeometryLoweringErrorKind::InvalidMaterial {
                detail: "active NIF stencil operations have no glTF material representation"
                    .to_owned(),
            },
        ));
    }
    Ok(Some(NativeMaterialSource {
        base_color,
        ambient: material.map_or([1.0; 3], |value| value.ambient),
        specular: material.map_or([0.0; 3], |value| value.specular),
        emissive,
        glossiness: material.map_or(0.0, |value| value.glossiness),
        base_texture,
        detail_texture,
        glow_texture,
        texture_apply_mode: texturing.map_or(2, |value| value.apply_mode),
        vertex_color_mode: vertex_color.map_or(0, |value| value.vertex_mode),
        lighting_mode: vertex_color.map_or(1, |value| value.lighting_mode),
        alpha_blend,
        source_blend_mode: alpha.map_or(0, |value| u32::from((value.flags & 0x001e) >> 1)),
        destination_blend_mode: alpha.map_or(0, |value| u32::from((value.flags & 0x01e0) >> 5)),
        alpha_cutoff,
        alpha_test_mode: alpha.map_or(0, |value| u32::from((value.flags & 0x1c00) >> 10)),
        depth_test: depth.is_none_or(|value| value.flags & 0x0001 != 0),
        depth_write: depth.is_none_or(|value| value.flags & 0x0002 != 0),
        depth_test_mode: depth.map_or(3, |value| value.function),
        cull_mode: stencil.map_or(0, |value| value.draw_mode),
    }))
}

pub(super) fn inherited_netimmerse_property_references(
    document: &NetImmerseNifDocument,
    geometry_block: u32,
    geometry: &NetImmerseNiGeometry,
) -> Result<Vec<i32>, NativeGeometryLoweringError> {
    let mut refs = geometry.av_object.property_refs.clone();
    let mut child = geometry_block as i32;
    let mut visited = HashSet::from([child]);
    while let Some(parent) = document
        .blocks()
        .find(|candidate| candidate.payload.child_and_effect_refs().0.contains(&child))
    {
        if !visited.insert(parent.index as i32) {
            return Err(native_geometry_lowering_error(
                NativeGeometrySourceFamily::Nif,
                &document.source_path,
                NativeGeometryLoweringErrorKind::InvalidMaterial {
                    detail: format!("cyclic property inheritance at NIF block {}", parent.index),
                },
            ));
        }
        if let Some(object) = parent.payload.av_object() {
            refs.extend(&object.property_refs);
        }
        child = parent.index as i32;
    }
    Ok(refs)
}
