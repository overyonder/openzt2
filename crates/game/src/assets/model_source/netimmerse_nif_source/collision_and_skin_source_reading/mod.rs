//! Compound transform, bounding-volume, and skin-partition reading for NIF records.

use super::{
    super::native_source_byte_reading::{
        read_f32_little_endian, read_source_boolean, read_three_by_three_matrix,
        read_three_component_vector, read_u16_little_endian, read_u32_little_endian,
    },
    collision_source_types::{
        NetImmerseBoundingVolume, NetImmerseBoxBoundingVolume, NetImmerseCapsuleBoundingVolume,
        NetImmerseHalfSpaceBoundingVolume, NetImmerseNiBound, NetImmerseNiPlane,
    },
    counted_source_collection_reading::{
        read_f32_value_grid, read_triangle_index_triplets, read_u16_values, read_u8_value_grid,
    },
    source_error::NetImmerseNifSourceError,
    transform_and_skin_source_types::{
        NetImmerseNiSkinBoneData, NetImmerseNiSkinWeight, NetImmerseNiTransform,
        NetImmerseSkinPartition,
    },
    NETIMMERSE_VERSION_10_1_0_0,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn read_transform(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<NetImmerseNiTransform> {
    Ok(NetImmerseNiTransform {
        rotation: read_three_by_three_matrix(source_bytes, cursor, source_path, field_name)?,
        translation: read_three_component_vector(source_bytes, cursor, source_path, field_name)?,
        scale: read_f32_little_endian(source_bytes, cursor, source_path, field_name)?,
    })
}

pub(super) fn read_bound(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<NetImmerseNiBound> {
    Ok(NetImmerseNiBound {
        center: read_three_component_vector(source_bytes, cursor, source_path, field_name)?,
        radius: read_f32_little_endian(source_bytes, cursor, source_path, field_name)?,
    })
}

pub(super) fn read_plane(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<NetImmerseNiPlane> {
    Ok(NetImmerseNiPlane {
        normal: read_three_component_vector(source_bytes, cursor, source_path, field_name)?,
        constant: read_f32_little_endian(source_bytes, cursor, source_path, field_name)?,
    })
}

pub(super) fn read_bounding_volume(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    field_name: &str,
) -> Result<NetImmerseBoundingVolume> {
    match read_u32_little_endian(source_bytes, cursor, source_path, field_name)? {
        0 => Ok(NetImmerseBoundingVolume::Sphere(read_bound(
            source_bytes,
            cursor,
            source_path,
            field_name,
        )?)),
        1 => Ok(NetImmerseBoundingVolume::Box(NetImmerseBoxBoundingVolume {
            center: read_three_component_vector(source_bytes, cursor, source_path, field_name)?,
            axes: (0..3)
                .map(|_| {
                    read_three_component_vector(source_bytes, cursor, source_path, field_name)
                        .map_err(Into::into)
                })
                .collect::<Result<Vec<_>>>()?,
            extent: read_three_component_vector(source_bytes, cursor, source_path, field_name)?,
        })),
        2 => Ok(NetImmerseBoundingVolume::Capsule(
            NetImmerseCapsuleBoundingVolume {
                center: read_three_component_vector(source_bytes, cursor, source_path, field_name)?,
                origin: read_three_component_vector(source_bytes, cursor, source_path, field_name)?,
                extent: read_f32_little_endian(source_bytes, cursor, source_path, field_name)?,
                radius: read_f32_little_endian(source_bytes, cursor, source_path, field_name)?,
            },
        )),
        4 => {
            let volume_count =
                read_u32_little_endian(source_bytes, cursor, source_path, field_name)?;
            (0..volume_count)
                .map(|_| read_bounding_volume(source_bytes, cursor, source_path, field_name))
                .collect::<Result<Vec<_>>>()
                .map(NetImmerseBoundingVolume::Union)
        }
        5 => Ok(NetImmerseBoundingVolume::HalfSpace(
            NetImmerseHalfSpaceBoundingVolume {
                plane: read_plane(source_bytes, cursor, source_path, field_name)?,
                center: read_three_component_vector(source_bytes, cursor, source_path, field_name)?,
            },
        )),
        unknown_kind => Ok(NetImmerseBoundingVolume::Unknown(unknown_kind)),
    }
}

pub(super) fn read_skin_bone_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    has_vertex_weights: bool,
    field_name: &str,
) -> Result<NetImmerseNiSkinBoneData> {
    let skin_transform = read_transform(source_bytes, cursor, source_path, field_name)?;
    let bounding_sphere = read_bound(source_bytes, cursor, source_path, field_name)?;
    let declared_vertex_count =
        read_u16_little_endian(source_bytes, cursor, source_path, field_name)?;
    let vertex_weights = if has_vertex_weights {
        (0..declared_vertex_count)
            .map(|_| {
                Ok(NetImmerseNiSkinWeight {
                    vertex_index: read_u16_little_endian(
                        source_bytes,
                        cursor,
                        source_path,
                        field_name,
                    )?,
                    weight: read_f32_little_endian(source_bytes, cursor, source_path, field_name)?,
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    Ok(NetImmerseNiSkinBoneData {
        skin_transform,
        bounding_sphere,
        vertex_weights,
    })
}

pub(super) fn read_skin_partition(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseSkinPartition> {
    let vertex_count = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "SkinPartition vertex count",
    )?;
    let triangle_count = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "SkinPartition triangle count",
    )?;
    let bone_count = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "SkinPartition bone count",
    )?;
    let strip_count = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "SkinPartition strip count",
    )?;
    let weights_per_vertex = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "SkinPartition weights per vertex",
    )?;
    let bones = read_u16_values(
        source_bytes,
        cursor,
        source_path,
        u32::from(bone_count),
        "SkinPartition bone",
    )?;
    let has_vertex_map = if encoded_version >= NETIMMERSE_VERSION_10_1_0_0 {
        read_source_boolean(
            source_bytes,
            cursor,
            source_path,
            "SkinPartition has vertex map",
        )?
    } else {
        true
    };
    let vertex_map = if has_vertex_map {
        read_u16_values(
            source_bytes,
            cursor,
            source_path,
            u32::from(vertex_count),
            "SkinPartition vertex map",
        )?
    } else {
        Vec::new()
    };
    let has_vertex_weights = if encoded_version >= NETIMMERSE_VERSION_10_1_0_0 {
        read_source_boolean(
            source_bytes,
            cursor,
            source_path,
            "SkinPartition has vertex weights",
        )?
    } else {
        true
    };
    let vertex_weights = if has_vertex_weights {
        read_f32_value_grid(
            source_bytes,
            cursor,
            source_path,
            u32::from(vertex_count),
            u32::from(weights_per_vertex),
            "SkinPartition vertex weight",
        )?
    } else {
        Vec::new()
    };
    let strip_lengths = read_u16_values(
        source_bytes,
        cursor,
        source_path,
        u32::from(strip_count),
        "SkinPartition strip length",
    )?;
    let has_faces = if encoded_version >= NETIMMERSE_VERSION_10_1_0_0 {
        read_source_boolean(source_bytes, cursor, source_path, "SkinPartition has faces")?
    } else {
        true
    };
    let (strips, triangles) = if has_faces && strip_count != 0 {
        let strips = strip_lengths
            .iter()
            .map(|strip_length| {
                read_u16_values(
                    source_bytes,
                    cursor,
                    source_path,
                    u32::from(*strip_length),
                    "SkinPartition strip index",
                )
            })
            .collect::<Result<Vec<_>>>()?;
        (strips, Vec::new())
    } else if has_faces {
        (
            Vec::new(),
            read_triangle_index_triplets(
                source_bytes,
                cursor,
                source_path,
                u32::from(triangle_count),
                "SkinPartition triangle",
            )?,
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let has_bone_indices = read_source_boolean(
        source_bytes,
        cursor,
        source_path,
        "SkinPartition has bone indices",
    )?;
    let bone_indices = has_bone_indices
        .then(|| {
            read_u8_value_grid(
                source_bytes,
                cursor,
                source_path,
                u32::from(vertex_count),
                u32::from(weights_per_vertex),
                "SkinPartition bone index",
            )
        })
        .transpose()?;
    Ok(NetImmerseSkinPartition {
        vertex_count,
        triangle_count,
        bone_count,
        strip_count,
        weights_per_vertex,
        bones,
        vertex_map,
        vertex_weights,
        strip_lengths,
        strips,
        triangles,
        bone_indices,
    })
}
