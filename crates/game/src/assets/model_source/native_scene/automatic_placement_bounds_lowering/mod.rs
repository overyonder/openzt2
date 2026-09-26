//! Automatic placement bounds computed from NIF/BFB geometry.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Result};

use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

use super::super::{
    blue_fang_bfb_source::{document::BlueFangBfbDocument, source_types::BlueFangBfbNodeKind},
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload,
        collision_source_types::{NetImmerseBoundingVolume, NetImmerseBoxBoundingVolume},
        document_source_types::NetImmerseNifDocument,
        scene_object_source_types::NetImmerseNiAvObject,
    },
};

pub(in crate::assets::model_source) fn lower_blue_fang_automatic_placement_bounds(
    document: &BlueFangBfbDocument,
) -> Result<Option<[[f32; 2]; 2]>> {
    let footprint_shape_block_ids = document
        .hierarchy()
        .filter(|node| node.name.eq_ignore_ascii_case("footprint"))
        .filter_map(|node| node.block_id.checked_add(1))
        .collect::<BTreeSet<_>>();
    let has_marker = !footprint_shape_block_ids.is_empty();
    let mut points = Vec::new();
    if has_marker {
        for collision in document
            .collision_boxes()
            .filter(|collision| footprint_shape_block_ids.contains(&collision.block_id))
        {
            if collision
                .half_extents
                .iter()
                .any(|extent| !extent.is_finite() || *extent <= 0.0)
            {
                bail!("authored BFB footprint box is nonfinite or empty");
            }
            for x in [-1.0_f32, 1.0] {
                for y in [-1.0_f32, 1.0] {
                    for z in [-1.0_f32, 1.0] {
                        points.push(blue_fang_bfb_point(
                            collision.transform,
                            [
                                collision.half_extents[0] * x,
                                collision.half_extents[1] * y,
                                collision.half_extents[2] * z,
                            ],
                        )?);
                    }
                }
            }
        }
    }
    if points.is_empty() {
        for node in document
            .hierarchy()
            .filter(|node| node.lod_index.is_none_or(|index| index == 0))
        {
            let mesh_block_id = match &node.kind {
                BlueFangBfbNodeKind::MeshLink { mesh_block_id, .. }
                | BlueFangBfbNodeKind::Billboard { mesh_block_id, .. } => *mesh_block_id,
                BlueFangBfbNodeKind::Node
                | BlueFangBfbNodeKind::LodGroup
                // Particle attachments have no static mesh vertices. Their
                // effects are lowered by the scene owner, not footprint bounds.
                | BlueFangBfbNodeKind::ParticleSystem { .. }
                | BlueFangBfbNodeKind::ModelJoint { .. }
                | BlueFangBfbNodeKind::Other(_) => continue,
            };
            let Some(mesh_block) = document
                .mesh_blocks()
                .find(|mesh_block| mesh_block.block_id == mesh_block_id)
            else {
                continue;
            };
            let Some(mesh) = document
                .meshes()
                .find(|mesh| mesh.block_id == mesh_block.mesh_data_block_id)
            else {
                continue;
            };
            let vertex_start = mesh_block.vertex_start as usize;
            let vertex_end = vertex_start + mesh_block.vertex_count as usize;
            for vertex in &mesh.vertices[vertex_start..vertex_end] {
                if vertex.position.iter().all(|value| value.is_finite()) {
                    points.push(blue_fang_bfb_point(node.world_transform, vertex.position)?);
                }
            }
        }
    }
    if points.is_empty() {
        points.extend(
            document
                .meshes()
                .flat_map(|mesh| &mesh.vertices)
                .filter(|vertex| vertex.position.iter().all(|value| value.is_finite()))
                .map(|vertex| convert_source_z_up_vector_to_bevy_y_up_coordinates(vertex.position)),
        );
    }
    automatic_placement_bounds_from_points(&points, has_marker)
}

pub(in crate::assets::model_source) fn lower_netimmerse_automatic_placement_bounds(
    document: &NetImmerseNifDocument,
) -> Result<Option<[[f32; 2]; 2]>> {
    let owners = document
        .blocks()
        .filter_map(|block| {
            block
                .payload
                .av_object()
                .filter(|object| object.object.name.eq_ignore_ascii_case("footprint"))
                .map(|_| block.index as i32)
        })
        .take(1)
        .collect::<Vec<_>>();
    let has_marker = !owners.is_empty();
    let parents = document
        .blocks()
        .flat_map(|block| {
            block
                .payload
                .child_and_effect_refs()
                .0
                .iter()
                .copied()
                .map(move |child| (child, block.index as i32))
        })
        .collect::<BTreeMap<_, _>>();
    let mut pending = if has_marker {
        owners
    } else {
        document
            .blocks()
            .filter(|block| {
                matches!(
                    block.payload,
                    NetImmerseNifBlockPayload::NiTriShape(_)
                        | NetImmerseNifBlockPayload::NiTriStrips(_)
                )
            })
            .map(|block| block.index as i32)
            .collect()
    };
    let mut seen = BTreeSet::new();
    let mut colliders = Vec::new();
    while let Some(block_reference) = pending.pop() {
        if !seen.insert(block_reference) {
            continue;
        }
        let block = document.block(block_reference).ok_or_else(|| {
            anyhow::anyhow!("authored footprint references a missing scene child")
        })?;
        pending.extend(block.payload.child_and_effect_refs().0.iter().copied());
        let Some(object) = block.payload.av_object() else {
            continue;
        };
        let Some(collision) = document.block(object.collision_object_ref) else {
            continue;
        };
        let NetImmerseNifBlockPayload::NiCollisionData(collision) = &collision.payload else {
            continue;
        };
        if let Some(volume) = collision.bounding_volume.as_ref() {
            collect_netimmerse_box_bounds(volume, block_reference, &mut colliders);
        }
    }

    let mut points = Vec::with_capacity(colliders.len().saturating_mul(8));
    if colliders.is_empty() {
        for block_reference in &seen {
            let block = document.block(*block_reference).ok_or_else(|| {
                anyhow::anyhow!("authored footprint references a missing scene child")
            })?;
            let data_reference = match &block.payload {
                NetImmerseNifBlockPayload::NiTriShape(shape) => shape.geometry.data_ref,
                NetImmerseNifBlockPayload::NiTriStrips(strips) => strips.geometry.data_ref,
                _ => continue,
            };
            let data = document
                .block(data_reference)
                .ok_or_else(|| anyhow::anyhow!("authored footprint geometry has no data block"))?;
            let vertices = match &data.payload {
                NetImmerseNifBlockPayload::NiTriShapeData(data) => data.geometry.vertices.as_ref(),
                NetImmerseNifBlockPayload::NiTriStripsData(data) => data.geometry.vertices.as_ref(),
                _ => None,
            };
            let Some(vertices) = vertices else {
                continue;
            };
            if vertices.iter().flatten().any(|value| !value.is_finite()) {
                bail!("authored footprint geometry is nonfinite");
            }
            let start = points.len();
            points.extend(vertices.iter().copied());
            transform_netimmerse_points(
                document,
                &parents,
                *block_reference,
                &mut points[start..],
            )?;
        }
    } else {
        for (owner, box_bounds) in colliders {
            let [axis_x, axis_y, axis_z] = <&[[f32; 3]; 3]>::try_from(box_bounds.axes.as_slice())
                .map_err(|_| {
                anyhow::anyhow!("authored footprint box does not have three axes")
            })?;
            if box_bounds
                .center
                .iter()
                .chain(box_bounds.extent.iter())
                .chain(axis_x.iter())
                .chain(axis_y.iter())
                .chain(axis_z.iter())
                .any(|value| !value.is_finite())
                || box_bounds.extent.iter().any(|extent| *extent <= 0.0)
            {
                bail!("authored footprint box is nonfinite or empty");
            }
            let start = points.len();
            for x in [-1.0_f32, 1.0] {
                for y in [-1.0_f32, 1.0] {
                    for z in [-1.0_f32, 1.0] {
                        points.push([
                            box_bounds.center[0]
                                + axis_x[0] * box_bounds.extent[0] * x
                                + axis_y[0] * box_bounds.extent[1] * y
                                + axis_z[0] * box_bounds.extent[2] * z,
                            box_bounds.center[1]
                                + axis_x[1] * box_bounds.extent[0] * x
                                + axis_y[1] * box_bounds.extent[1] * y
                                + axis_z[1] * box_bounds.extent[2] * z,
                            box_bounds.center[2]
                                + axis_x[2] * box_bounds.extent[0] * x
                                + axis_y[2] * box_bounds.extent[1] * y
                                + axis_z[2] * box_bounds.extent[2] * z,
                        ]);
                    }
                }
            }
            transform_netimmerse_points(document, &parents, owner, &mut points[start..])?;
        }
    }
    automatic_placement_bounds_from_points(&points, has_marker)
}

fn collect_netimmerse_box_bounds<'a>(
    volume: &'a NetImmerseBoundingVolume,
    owner: i32,
    output: &mut Vec<(i32, &'a NetImmerseBoxBoundingVolume)>,
) {
    match volume {
        NetImmerseBoundingVolume::Box(box_bounds) => output.push((owner, box_bounds)),
        NetImmerseBoundingVolume::Union(children) => children
            .iter()
            .for_each(|child| collect_netimmerse_box_bounds(child, owner, output)),
        NetImmerseBoundingVolume::Sphere(_)
        | NetImmerseBoundingVolume::Capsule(_)
        | NetImmerseBoundingVolume::HalfSpace(_)
        | NetImmerseBoundingVolume::Unknown(_) => {}
    }
}

fn transform_netimmerse_points(
    document: &NetImmerseNifDocument,
    parents: &BTreeMap<i32, i32>,
    owner: i32,
    points: &mut [[f32; 3]],
) -> Result<()> {
    let mut current = Some(owner);
    let mut visited = BTreeSet::new();
    while let Some(block_reference) = current {
        if !visited.insert(block_reference) {
            bail!("authored footprint scene ancestry is cyclic");
        }
        let object = document
            .block(block_reference)
            .and_then(|block| block.payload.av_object())
            .ok_or_else(|| anyhow::anyhow!("authored footprint transform is invalid"))?;
        for point in &mut *points {
            *point = transform_netimmerse_point(object, *point);
        }
        current = parents.get(&block_reference).copied();
    }
    points.iter_mut().for_each(|point| {
        *point = convert_source_z_up_vector_to_bevy_y_up_coordinates(*point);
    });
    Ok(())
}

fn transform_netimmerse_point(object: &NetImmerseNiAvObject, point: [f32; 3]) -> [f32; 3] {
    let point = point.map(|value| value * object.scale);
    [
        point[0] * object.rotation[0]
            + point[1] * object.rotation[1]
            + point[2] * object.rotation[2]
            + object.translation[0],
        point[0] * object.rotation[3]
            + point[1] * object.rotation[4]
            + point[2] * object.rotation[5]
            + object.translation[1],
        point[0] * object.rotation[6]
            + point[1] * object.rotation[7]
            + point[2] * object.rotation[8]
            + object.translation[2],
    ]
}

fn blue_fang_bfb_point(matrix: [[f32; 4]; 4], point: [f32; 3]) -> Result<[f32; 3]> {
    if matrix.iter().flatten().any(|value| !value.is_finite()) {
        bail!("authored BFB footprint transform is invalid");
    }
    Ok(convert_source_z_up_vector_to_bevy_y_up_coordinates([
        point[0] * matrix[0][0] + point[1] * matrix[1][0] + point[2] * matrix[2][0] + matrix[3][0],
        point[0] * matrix[0][1] + point[1] * matrix[1][1] + point[2] * matrix[2][1] + matrix[3][1],
        point[0] * matrix[0][2] + point[1] * matrix[1][2] + point[2] * matrix[2][2] + matrix[3][2],
    ]))
}

fn automatic_placement_bounds_from_points(
    points: &[[f32; 3]],
    has_marker: bool,
) -> Result<Option<[[f32; 2]; 2]>> {
    if points.is_empty() {
        return Ok(None);
    }
    let minimum_xz = points.iter().fold([f32::INFINITY; 2], |minimum, point| {
        [minimum[0].min(point[0]), minimum[1].min(point[2])]
    });
    let maximum_xz = points
        .iter()
        .fold([f32::NEG_INFINITY; 2], |maximum, point| {
            [maximum[0].max(point[0]), maximum[1].max(point[2])]
        });
    if minimum_xz
        .iter()
        .chain(maximum_xz.iter())
        .any(|value| !value.is_finite())
        || (0..2).any(|axis| maximum_xz[axis] < minimum_xz[axis])
    {
        if has_marker {
            bail!("authored footprint horizontal bounds are invalid");
        }
        return Ok(None);
    }
    Ok(Some([minimum_xz, maximum_xz]))
}
