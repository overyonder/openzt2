//! Focused parsing of NIF vertex channels and triangle topology records.

use super::{
    super::native_source_byte_reading::{
        read_f32_little_endian, read_four_component_colour, read_source_boolean,
        read_three_component_vector, read_three_component_vectors, read_u16_little_endian,
        read_u32_little_endian, read_u8,
    },
    counted_source_collection_reading::{read_triangle_index_triplets, read_u16_values},
    geometry_data_source_types::{
        NetImmerseMatchGroup, NetImmerseNiGeometryData, NetImmerseNiTriShapeData,
        NetImmerseNiTriStripsData,
    },
    source_error::NetImmerseNifSourceError,
    NETIMMERSE_VERSION_10_0_1_3, NETIMMERSE_VERSION_10_1_0_0,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn parse_geometry_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiGeometryData> {
    let num_vertices = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiGeometryData vertex count",
    )?;
    let vertices = read_source_boolean(
        source_bytes,
        cursor,
        source_path,
        "NiGeometryData has vertices",
    )?
    .then(|| {
        read_three_component_vectors(
            source_bytes,
            cursor,
            source_path,
            num_vertices,
            "NiGeometryData vertex",
        )
        .map_err(NetImmerseNifSourceError::from)
    })
    .transpose()?;
    let num_uv_sets = read_u8(
        source_bytes,
        cursor,
        source_path,
        "NiGeometryData UV set count",
    )?;
    let extra_vectors_flags = read_u8(
        source_bytes,
        cursor,
        source_path,
        "NiGeometryData extra vectors flags",
    )?;
    let normals = read_source_boolean(
        source_bytes,
        cursor,
        source_path,
        "NiGeometryData has normals",
    )?
    .then(|| {
        read_three_component_vectors(
            source_bytes,
            cursor,
            source_path,
            num_vertices,
            "NiGeometryData normal",
        )
        .map_err(NetImmerseNifSourceError::from)
    })
    .transpose()?;
    let center =
        read_three_component_vector(source_bytes, cursor, source_path, "NiGeometryData center")?;
    let radius =
        read_f32_little_endian(source_bytes, cursor, source_path, "NiGeometryData radius")?;
    let vertex_colors = read_source_boolean(
        source_bytes,
        cursor,
        source_path,
        "NiGeometryData has colors",
    )?
    .then(|| {
        (0..num_vertices)
            .map(|_| {
                read_four_component_colour(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiGeometryData color",
                )
                .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()
    })
    .transpose()?;
    let uv_sets = (0..(num_uv_sets & 63))
        .map(|_| {
            (0..num_vertices)
                .map(|_| {
                    Ok([
                        read_f32_little_endian(
                            source_bytes,
                            cursor,
                            source_path,
                            "NiGeometryData u",
                        )?,
                        read_f32_little_endian(
                            source_bytes,
                            cursor,
                            source_path,
                            "NiGeometryData v",
                        )?,
                    ])
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    let consistency_flags = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiGeometryData consistency flags",
    )?;
    Ok(NetImmerseNiGeometryData {
        num_vertices,
        vertices,
        num_uv_sets,
        extra_vectors_flags,
        normals,
        center,
        radius,
        vertex_colors,
        uv_sets,
        consistency_flags,
    })
}

pub(super) fn parse_triangle_strips_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiTriStripsData> {
    let geometry = parse_geometry_data(source_bytes, cursor, source_path)?;
    let num_triangles = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTriStripsData triangle count",
    )?;
    let strip_count = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTriStripsData strip count",
    )?;
    let strip_lengths = read_u16_values(
        source_bytes,
        cursor,
        source_path,
        u32::from(strip_count),
        "NiTriStripsData strip length",
    )?;
    let has_points = if encoded_version >= NETIMMERSE_VERSION_10_0_1_3 {
        read_source_boolean(
            source_bytes,
            cursor,
            source_path,
            "NiTriStripsData has points",
        )?
    } else {
        true
    };
    let points = has_points
        .then(|| {
            strip_lengths
                .iter()
                .map(|strip_length| {
                    read_u16_values(
                        source_bytes,
                        cursor,
                        source_path,
                        u32::from(*strip_length),
                        "NiTriStripsData point",
                    )
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;
    Ok(NetImmerseNiTriStripsData {
        geometry,
        num_triangles,
        strip_lengths,
        points,
    })
}

pub(super) fn parse_triangle_shape_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
    encoded_version: u32,
) -> Result<NetImmerseNiTriShapeData> {
    let geometry = parse_geometry_data(source_bytes, cursor, source_path)?;
    let num_triangles = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTriShapeData triangle count",
    )?;
    let num_triangle_points = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTriShapeData point count",
    )?;
    let has_triangles = if encoded_version >= NETIMMERSE_VERSION_10_1_0_0 {
        read_source_boolean(
            source_bytes,
            cursor,
            source_path,
            "NiTriShapeData has triangles",
        )?
    } else {
        true
    };
    let triangles = has_triangles
        .then(|| {
            read_triangle_index_triplets(
                source_bytes,
                cursor,
                source_path,
                u32::from(num_triangles),
                "NiTriShapeData triangle",
            )
        })
        .transpose()?;
    let match_group_count = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiTriShapeData match group count",
    )?;
    let match_groups = (0..match_group_count)
        .map(|_| {
            let vertex_count = read_u16_little_endian(
                source_bytes,
                cursor,
                source_path,
                "NiTriShapeData match group size",
            )?;
            Ok(NetImmerseMatchGroup {
                vertex_indices: read_u16_values(
                    source_bytes,
                    cursor,
                    source_path,
                    u32::from(vertex_count),
                    "NiTriShapeData match group vertex",
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiTriShapeData {
        geometry,
        num_triangles,
        num_triangle_points,
        triangles,
        match_groups,
    })
}
