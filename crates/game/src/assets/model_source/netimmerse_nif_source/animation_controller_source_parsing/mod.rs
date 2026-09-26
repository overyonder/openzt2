//! Focused parsing of NIF time controllers, animation data, morphs, and LOD controllers.

use super::{
    super::native_source_byte_reading::{
        read_f32_little_endian, read_i32_little_endian, read_sized_string, read_u16_little_endian,
        read_u32_counted_three_component_vectors, read_u32_little_endian, read_u8,
    },
    animation_controller_source_types::{
        NetImmerseControlledBlock, NetImmerseMorph, NetImmerseNiAlphaController,
        NetImmerseNiBoneLodController, NetImmerseNiBoneLodSkinInfo, NetImmerseNiControllerSequence,
        NetImmerseNiGeomMorpherController, NetImmerseNiKeyframeController,
        NetImmerseNiKeyframeData, NetImmerseNiMaterialColorController, NetImmerseNiMorphData,
        NetImmerseNiTextKeyExtraData, NetImmerseNiTimeController, NetImmerseNiUvController,
        NetImmerseNiVisController, NetImmerseTextKey,
    },
    counted_source_collection_reading::read_i32_values,
    interpolated_key_source_reading::{
        read_float_key_group, read_float_keys, read_quaternion_keys, read_vector3_key_group,
    },
    source_error::NetImmerseNifSourceError,
    NETIMMERSE_VERSION_10_0_1_3, NETIMMERSE_VERSION_10_1_0_0,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn parse_time_controller(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiTimeController> {
    Ok(NetImmerseNiTimeController {
        next_controller_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTimeController next ref",
        )?,
        flags: read_u16_little_endian(source_bytes, cursor, source_path, "NiTimeController flags")?,
        frequency: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTimeController frequency",
        )?,
        phase: read_f32_little_endian(source_bytes, cursor, source_path, "NiTimeController phase")?,
        start_time: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTimeController start time",
        )?,
        stop_time: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTimeController stop time",
        )?,
        target_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiTimeController target ref",
        )?,
    })
}

pub(super) fn parse_alpha_controller(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiAlphaController> {
    Ok(NetImmerseNiAlphaController {
        controller: parse_time_controller(source_bytes, cursor, source_path)?,
        data_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiAlphaController data ref",
        )?,
    })
}

pub(super) fn parse_keyframe_controller(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiKeyframeController> {
    Ok(NetImmerseNiKeyframeController {
        controller: parse_time_controller(source_bytes, cursor, source_path)?,
        data_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiKeyframeController data ref",
        )?,
    })
}

pub(super) fn parse_controller_sequence(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiControllerSequence> {
    if encoded_version > NETIMMERSE_VERSION_10_1_0_0 {
        return Err(NetImmerseNifSourceError::invalid_data(
            source_path,
            format!(
                "NiControllerSequence version {encoded_version:#010x} uses an unsupported controlled-block layout"
            ),
        ));
    }
    let name = read_sized_string(
        source_bytes,
        cursor,
        source_path,
        "NiControllerSequence name",
    )?;
    let accum_root_name = read_sized_string(
        source_bytes,
        cursor,
        source_path,
        "NiControllerSequence accumulation root",
    )?;
    let text_keys_ref = read_i32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiControllerSequence text keys",
    )?;
    let controlled_block_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiControllerSequence controlled block count",
    )?;
    let controlled_blocks = (0..controlled_block_count)
        .map(|_| {
            Ok(NetImmerseControlledBlock {
                target_name: read_sized_string(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiControllerSequence target name",
                )?,
                controller_ref: read_i32_little_endian(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiControllerSequence controller",
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiControllerSequence {
        name,
        accum_root_name,
        text_keys_ref,
        controlled_blocks,
    })
}

pub(super) fn parse_material_colour_controller(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiMaterialColorController> {
    let controller = parse_time_controller(source_bytes, cursor, source_path)?;
    let target_color = if encoded_version >= NETIMMERSE_VERSION_10_1_0_0 {
        Some(read_u32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiMaterialColorController target color",
        )?)
    } else {
        Some(u32::from((controller.flags >> 4) & 0b11))
    };
    Ok(NetImmerseNiMaterialColorController {
        controller,
        target_color,
        data_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiMaterialColorController data ref",
        )?,
    })
}

pub(super) fn parse_uv_controller(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiUvController> {
    Ok(NetImmerseNiUvController {
        controller: parse_time_controller(source_bytes, cursor, source_path)?,
        texture_set: read_u16_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiUVController texture set",
        )?,
        data_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiUVController data ref",
        )?,
    })
}

pub(super) fn parse_geometry_morpher_controller(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiGeomMorpherController> {
    Ok(NetImmerseNiGeomMorpherController {
        controller: parse_time_controller(source_bytes, cursor, source_path)?,
        morpher_flags: (encoded_version >= NETIMMERSE_VERSION_10_0_1_3)
            .then(|| {
                read_u16_little_endian(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiGeomMorpherController flags",
                )
            })
            .transpose()?,
        data_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiGeomMorpherController data ref",
        )?,
        always_update: Some(read_u8(
            source_bytes,
            cursor,
            source_path,
            "NiGeomMorpherController always update",
        )?),
    })
}

pub(super) fn parse_visibility_controller(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiVisController> {
    Ok(NetImmerseNiVisController {
        controller: parse_time_controller(source_bytes, cursor, source_path)?,
        data_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiVisController data ref",
        )?,
    })
}

pub(super) fn parse_keyframe_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiKeyframeData> {
    let num_rotation_keys = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiKeyframeData rotation count",
    )?;
    let rotation_type = (num_rotation_keys != 0)
        .then(|| {
            read_u32_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiKeyframeData rotation type",
            )
        })
        .transpose()?;
    let quaternion_keys = if rotation_type.is_some_and(|value| value != 4) {
        read_quaternion_keys(
            source_bytes,
            cursor,
            source_path,
            num_rotation_keys,
            rotation_type.unwrap_or_default(),
            "NiKeyframeData rotation key",
        )?
    } else {
        Vec::new()
    };
    let order = (rotation_type == Some(4) && encoded_version <= NETIMMERSE_VERSION_10_1_0_0)
        .then(|| {
            read_f32_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiKeyframeData XYZ order",
            )
        })
        .transpose()?;
    let xyz_rotations = if rotation_type == Some(4) {
        (0..3)
            .map(|_| {
                read_float_key_group(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiKeyframeData XYZ rotation",
                )
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    Ok(NetImmerseNiKeyframeData {
        num_rotation_keys,
        rotation_type,
        quaternion_keys,
        order,
        xyz_rotations,
        translations: read_vector3_key_group(
            source_bytes,
            cursor,
            source_path,
            "NiKeyframeData translations",
        )?,
        scales: read_float_key_group(source_bytes, cursor, source_path, "NiKeyframeData scales")?,
    })
}

pub(super) fn parse_morph_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiMorphData> {
    let num_morphs =
        read_u32_little_endian(source_bytes, cursor, source_path, "NiMorphData morph count")?;
    let num_vertices = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiMorphData vertex count",
    )?;
    let relative_targets = read_u8(
        source_bytes,
        cursor,
        source_path,
        "NiMorphData relative targets",
    )?;
    let morphs = (0..num_morphs)
        .map(|_| {
            let num_keys =
                read_u32_little_endian(source_bytes, cursor, source_path, "NiMorphData key count")?;
            let interpolation = read_u32_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiMorphData interpolation",
            )?;
            Ok(NetImmerseMorph {
                num_keys,
                interpolation,
                keys: read_float_keys(
                    source_bytes,
                    cursor,
                    source_path,
                    num_keys,
                    interpolation,
                    "NiMorphData key",
                )?,
                vectors: read_u32_counted_three_component_vectors(
                    source_bytes,
                    cursor,
                    source_path,
                    num_vertices,
                    "NiMorphData vector",
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiMorphData {
        num_morphs,
        num_vertices,
        relative_targets,
        morphs,
    })
}

pub(super) fn parse_text_key_extra_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiTextKeyExtraData> {
    let name = read_sized_string(source_bytes, cursor, source_path, "NiTextKeyExtraData name")?;
    let key_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTextKeyExtraData key count",
    )?;
    let keys = (0..key_count)
        .map(|_| {
            Ok(NetImmerseTextKey {
                time: read_f32_little_endian(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiTextKeyExtraData key time",
                )?,
                value: read_sized_string(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiTextKeyExtraData key value",
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiTextKeyExtraData { name, keys })
}

pub(super) fn parse_bone_level_of_detail_controller(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiBoneLodController> {
    let controller = parse_time_controller(source_bytes, cursor, source_path)?;
    let lod = read_u32_little_endian(source_bytes, cursor, source_path, "NiBoneLODController lod")?;
    let lod_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiBoneLODController lod count",
    )?;
    let _declared_node_group_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiBoneLODController node group count",
    )?;
    let node_groups = (0..lod_count)
        .map(|_| {
            let node_count = read_u32_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiBoneLODController node count",
            )?;
            read_i32_values(
                source_bytes,
                cursor,
                source_path,
                node_count,
                "NiBoneLODController node ref",
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let shape_group_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiBoneLODController shape group count",
    )?;
    let shape_groups = (0..shape_group_count)
        .map(|_| {
            let skin_info_count = read_u32_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiBoneLODController skin info count",
            )?;
            (0..skin_info_count)
                .map(|_| {
                    Ok(NetImmerseNiBoneLodSkinInfo {
                        shape_ref: read_i32_little_endian(
                            source_bytes,
                            cursor,
                            source_path,
                            "NiBoneLODController shape ref",
                        )?,
                        skin_instance_ref: read_i32_little_endian(
                            source_bytes,
                            cursor,
                            source_path,
                            "NiBoneLODController skin instance ref",
                        )?,
                    })
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    let shape_reference_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiBoneLODController shape ref count",
    )?;
    let shape_refs = read_i32_values(
        source_bytes,
        cursor,
        source_path,
        shape_reference_count,
        "NiBoneLODController shape group ref",
    )?;
    Ok(NetImmerseNiBoneLodController {
        controller,
        lod,
        node_groups,
        shape_groups,
        shape_refs,
    })
}
