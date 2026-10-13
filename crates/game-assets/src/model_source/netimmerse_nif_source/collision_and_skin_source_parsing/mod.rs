//! Focused parsing of NIF collision data, skin data, instances, and partitions.

use super::{
    super::native_source_byte_reading::{
        read_i32_little_endian, read_source_boolean, read_u32_little_endian, read_u8,
    },
    collision_and_skin_source_reading::{
        read_bounding_volume, read_skin_bone_data, read_skin_partition, read_transform,
    },
    collision_source_types::{NetImmerseNiCollisionData, NetImmerseNiCollisionObject},
    counted_source_collection_reading::read_i32_values,
    source_error::NetImmerseNifSourceError,
    transform_and_skin_source_types::{
        NetImmerseNiSkinData, NetImmerseNiSkinInstance, NetImmerseNiSkinPartition,
    },
    NETIMMERSE_VERSION_10_1_0_0,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn parse_collision_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiCollisionData> {
    let object = NetImmerseNiCollisionObject {
        target_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiCollisionObject target ref",
        )?,
    };
    let propagation_mode = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiCollisionData propagation mode",
    )?;
    let collision_mode = (encoded_version >= NETIMMERSE_VERSION_10_1_0_0)
        .then(|| {
            read_u32_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiCollisionData collision mode",
            )
        })
        .transpose()?;
    let use_abv = read_u8(source_bytes, cursor, source_path, "NiCollisionData use ABV")?;
    let bounding_volume = (use_abv == 1)
        .then(|| {
            read_bounding_volume(
                source_bytes,
                cursor,
                source_path,
                "NiCollisionData bounding volume",
            )
        })
        .transpose()?;
    Ok(NetImmerseNiCollisionData {
        object,
        propagation_mode,
        collision_mode,
        use_abv,
        bounding_volume,
    })
}

pub(super) fn parse_skin_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiSkinData> {
    let skin_transform = read_transform(
        source_bytes,
        cursor,
        source_path,
        "NiSkinData skin transform",
    )?;
    let bone_count =
        read_u32_little_endian(source_bytes, cursor, source_path, "NiSkinData bone count")?;
    let skin_partition_ref = (encoded_version < NETIMMERSE_VERSION_10_1_0_0)
        .then(|| {
            read_i32_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiSkinData skin partition ref",
            )
        })
        .transpose()?;
    let has_vertex_weights = read_source_boolean(
        source_bytes,
        cursor,
        source_path,
        "NiSkinData has vertex weights",
    )?;
    let bones = (0..bone_count)
        .map(|_| {
            read_skin_bone_data(
                source_bytes,
                cursor,
                source_path,
                has_vertex_weights,
                "NiSkinData bone",
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiSkinData {
        skin_transform,
        skin_partition_ref,
        has_vertex_weights,
        bones,
    })
}

pub(super) fn parse_skin_instance(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiSkinInstance> {
    let data_ref =
        read_i32_little_endian(source_bytes, cursor, source_path, "NiSkinInstance data ref")?;
    let skin_partition_ref = (encoded_version >= NETIMMERSE_VERSION_10_1_0_0)
        .then(|| {
            read_i32_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiSkinInstance skin partition ref",
            )
        })
        .transpose()?;
    let skeleton_root_ref = read_i32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiSkinInstance skeleton root ref",
    )?;
    let bone_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiSkinInstance bone count",
    )?;
    let bone_refs = read_i32_values(
        source_bytes,
        cursor,
        source_path,
        bone_count,
        "NiSkinInstance bone ref",
    )?;
    Ok(NetImmerseNiSkinInstance {
        data_ref,
        skin_partition_ref,
        skeleton_root_ref,
        bone_refs,
    })
}

pub(super) fn parse_skin_partition_collection(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiSkinPartition> {
    let partition_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiSkinPartition partition count",
    )?;
    let partitions = (0..partition_count)
        .map(|_| read_skin_partition(source_bytes, cursor, source_path, encoded_version))
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiSkinPartition { partitions })
}
