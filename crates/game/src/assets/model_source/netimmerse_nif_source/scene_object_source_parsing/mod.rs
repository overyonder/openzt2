//! Focused parsing of NIF scene objects, nodes, lights, and geometry objects.

use super::{
    super::native_source_byte_reading::{
        read_f32_little_endian, read_i32_little_endian, read_sized_string, read_source_boolean,
        read_three_by_three_matrix, read_three_component_vector, read_u16_little_endian,
        read_u32_little_endian,
    },
    counted_source_collection_reading::read_i32_values,
    scene_object_source_types::{
        NetImmerseLodRange, NetImmerseNiAmbientLight, NetImmerseNiAvObject,
        NetImmerseNiBillboardNode, NetImmerseNiDirectionalLight, NetImmerseNiGeometry,
        NetImmerseNiGeometryShader, NetImmerseNiLight, NetImmerseNiLodNode, NetImmerseNiNode,
        NetImmerseNiObjectNet, NetImmerseNiParticleMeshes, NetImmerseNiParticles,
        NetImmerseNiPointLight, NetImmerseNiSwitchNode, NetImmerseNiTriShape,
        NetImmerseNiTriStrips,
    },
    source_error::NetImmerseNifSourceError,
    NETIMMERSE_VERSION_10_1_0_0,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn parse_object_identity_and_controller_links(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiObjectNet> {
    let name = read_sized_string(source_bytes, cursor, source_path, "NiObjectNET name")?;
    let extra_data_reference_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiObjectNET extra-data count",
    )?;
    let extra_data_refs = read_i32_values(
        source_bytes,
        cursor,
        source_path,
        extra_data_reference_count,
        "NiObjectNET extra-data ref",
    )?;
    let controller_ref = read_i32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiObjectNET controller ref",
    )?;
    Ok(NetImmerseNiObjectNet {
        name,
        extra_data_refs,
        controller_ref,
    })
}

pub(super) fn parse_transformable_scene_object(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiAvObject> {
    let object = parse_object_identity_and_controller_links(source_bytes, cursor, source_path)?;
    let flags = read_u16_little_endian(source_bytes, cursor, source_path, "NiAVObject flags")?;
    let translation =
        read_three_component_vector(source_bytes, cursor, source_path, "NiAVObject translation")?;
    let rotation =
        read_three_by_three_matrix(source_bytes, cursor, source_path, "NiAVObject rotation")?;
    let scale = read_f32_little_endian(source_bytes, cursor, source_path, "NiAVObject scale")?;
    let property_reference_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiAVObject property count",
    )?;
    let property_refs = read_i32_values(
        source_bytes,
        cursor,
        source_path,
        property_reference_count,
        "NiAVObject property ref",
    )?;
    let collision_object_ref = read_i32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiAVObject collision ref",
    )?;
    Ok(NetImmerseNiAvObject {
        object,
        flags,
        translation,
        rotation,
        scale,
        property_refs,
        collision_object_ref,
    })
}

fn parse_light(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiLight> {
    Ok(NetImmerseNiLight {
        av_object: parse_transformable_scene_object(source_bytes, cursor, source_path)?,
        dimmer: read_f32_little_endian(source_bytes, cursor, source_path, "NiLight dimmer")?,
        ambient: read_three_component_vector(source_bytes, cursor, source_path, "NiLight ambient")?,
        diffuse: read_three_component_vector(source_bytes, cursor, source_path, "NiLight diffuse")?,
        specular: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiLight specular",
        )?,
    })
}

pub(super) fn parse_directional_light(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiDirectionalLight> {
    Ok(NetImmerseNiDirectionalLight {
        light: parse_light(source_bytes, cursor, source_path)?,
    })
}

pub(super) fn parse_ambient_light(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiAmbientLight> {
    Ok(NetImmerseNiAmbientLight {
        light: parse_light(source_bytes, cursor, source_path)?,
    })
}

pub(super) fn parse_point_light(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiPointLight> {
    Ok(NetImmerseNiPointLight {
        light: parse_light(source_bytes, cursor, source_path)?,
        constant_attenuation: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiPointLight constant attenuation",
        )?,
        linear_attenuation: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiPointLight linear attenuation",
        )?,
        quadratic_attenuation: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiPointLight quadratic attenuation",
        )?,
    })
}

pub(super) fn parse_grouping_node(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiNode> {
    let av_object = parse_transformable_scene_object(source_bytes, cursor, source_path)?;
    let child_reference_count =
        read_u32_little_endian(source_bytes, cursor, source_path, "NiNode child count")?;
    let child_refs = read_i32_values(
        source_bytes,
        cursor,
        source_path,
        child_reference_count,
        "NiNode child ref",
    )?;
    let effect_reference_count =
        read_u32_little_endian(source_bytes, cursor, source_path, "NiNode effect count")?;
    let effect_refs = read_i32_values(
        source_bytes,
        cursor,
        source_path,
        effect_reference_count,
        "NiNode effect ref",
    )?;
    Ok(NetImmerseNiNode {
        av_object,
        child_refs,
        effect_refs,
    })
}

pub(super) fn parse_billboard_node(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiBillboardNode> {
    let node = parse_grouping_node(source_bytes, cursor, source_path)?;
    let billboard_mode = (encoded_version >= NETIMMERSE_VERSION_10_1_0_0)
        .then(|| read_u32_little_endian(source_bytes, cursor, source_path, "NiBillboardNode mode"))
        .transpose()?;
    Ok(NetImmerseNiBillboardNode {
        node,
        billboard_mode,
    })
}

fn parse_switch_node(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiSwitchNode> {
    let node = parse_grouping_node(source_bytes, cursor, source_path)?;
    let switch_flags = (encoded_version >= NETIMMERSE_VERSION_10_1_0_0)
        .then(|| read_u16_little_endian(source_bytes, cursor, source_path, "NiSwitchNode flags"))
        .transpose()?;
    let active_child_index = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiSwitchNode active child",
    )?;
    Ok(NetImmerseNiSwitchNode {
        node,
        switch_flags,
        active_child_index,
    })
}

pub(super) fn parse_level_of_detail_node(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiLodNode> {
    let switch_node = parse_switch_node(source_bytes, cursor, source_path, encoded_version)?;
    if encoded_version < NETIMMERSE_VERSION_10_1_0_0 {
        let lod_center =
            read_three_component_vector(source_bytes, cursor, source_path, "NiLODNode center")?;
        let level_count =
            read_u32_little_endian(source_bytes, cursor, source_path, "NiLODNode level count")?;
        let lod_levels = (0..level_count)
            .map(|_| {
                Ok(NetImmerseLodRange {
                    near_extent: read_f32_little_endian(
                        source_bytes,
                        cursor,
                        source_path,
                        "NiLODNode near extent",
                    )?,
                    far_extent: read_f32_little_endian(
                        source_bytes,
                        cursor,
                        source_path,
                        "NiLODNode far extent",
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        return Ok(NetImmerseNiLodNode {
            switch_node,
            lod_center: Some(lod_center),
            lod_levels,
            lod_level_data_ref: None,
        });
    }
    Ok(NetImmerseNiLodNode {
        switch_node,
        lod_center: None,
        lod_levels: Vec::new(),
        lod_level_data_ref: Some(read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiLODNode level data ref",
        )?),
    })
}

fn parse_geometry_object(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiGeometry> {
    let av_object = parse_transformable_scene_object(source_bytes, cursor, source_path)?;
    let data_ref =
        read_i32_little_endian(source_bytes, cursor, source_path, "NiGeometry data ref")?;
    let skin_instance_ref =
        read_i32_little_endian(source_bytes, cursor, source_path, "NiGeometry skin ref")?;
    let shader = read_source_boolean(source_bytes, cursor, source_path, "NiGeometry has shader")?
        .then(|| {
            Ok::<_, NetImmerseNifSourceError>(NetImmerseNiGeometryShader {
                name: read_sized_string(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiGeometry shader name",
                )?,
                extra_data: read_i32_little_endian(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiGeometry shader extra data",
                )?,
            })
        })
        .transpose()?;
    Ok(NetImmerseNiGeometry {
        av_object,
        data_ref,
        skin_instance_ref,
        shader,
    })
}

pub(super) fn parse_particles_geometry(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticles> {
    Ok(NetImmerseNiParticles {
        geometry: parse_geometry_object(source_bytes, cursor, source_path)?,
    })
}

pub(super) fn parse_triangle_strips_geometry(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiTriStrips> {
    Ok(NetImmerseNiTriStrips {
        geometry: parse_geometry_object(source_bytes, cursor, source_path)?,
    })
}

pub(super) fn parse_triangle_shape_geometry(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiTriShape> {
    Ok(NetImmerseNiTriShape {
        geometry: parse_geometry_object(source_bytes, cursor, source_path)?,
    })
}

pub(super) fn parse_particle_meshes_geometry(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleMeshes> {
    Ok(NetImmerseNiParticleMeshes {
        geometry: parse_geometry_object(source_bytes, cursor, source_path)?,
    })
}
