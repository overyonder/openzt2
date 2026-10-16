//! Automatic placement bounds computed from NIF/BFB geometry.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Result};

use crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

use super::super::{
    blue_fang_bfb_source::{
        document::BlueFangBfbDocument,
        source_types::{BlueFangBfbNode, BlueFangBfbNodeKind},
    },
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload,
        collision_source_types::{NetImmerseBoundingVolume, NetImmerseBoxBoundingVolume},
        document_source_types::NetImmerseNifDocument,
        scene_object_source_types::NetImmerseNiAvObject,
    },
};

pub(in crate::model_source) fn lower_blue_fang_automatic_placement_bounds(
    document: &BlueFangBfbDocument,
) -> Result<Option<[[f32; 2]; 2]>> {
    // The footprint node's shape block follows it. The box is placed in that
    // node's space, whose scale and rotation apply to it.
    let footprint_node_transforms_by_shape_block = document
        .hierarchy()
        .filter(|node| node.name.eq_ignore_ascii_case("footprint"))
        .filter_map(|node| Some((node.block_id.checked_add(1)?, node.world_transform)))
        .collect::<BTreeMap<_, _>>();
    let has_marker = !footprint_node_transforms_by_shape_block.is_empty();
    let mut points = Vec::new();
    if has_marker {
        for collision in document.collision_boxes() {
            let Some(footprint_node_transform) =
                footprint_node_transforms_by_shape_block.get(&collision.block_id)
            else {
                continue;
            };
            if collision
                .half_extents
                .iter()
                .any(|extent| !extent.is_finite() || *extent <= 0.0)
            {
                bail!("authored BFB footprint box is nonfinite or empty");
            }
            // The stored sizes are the box's full edge lengths: a hollow
            // log's box then spans its 4.1 m mesh and rests on the ground.
            for x in [-0.5_f32, 0.5] {
                for y in [-0.5_f32, 0.5] {
                    for z in [-0.5_f32, 0.5] {
                        let in_footprint_node = transform_blue_fang_bfb_source_point(
                            collision.transform,
                            [
                                collision.half_extents[0] * x,
                                collision.half_extents[1] * y,
                                collision.half_extents[2] * z,
                            ],
                        )?;
                        points.push(blue_fang_bfb_point(
                            *footprint_node_transform,
                            in_footprint_node,
                        )?);
                    }
                }
            }
        }
    }
    if points.is_empty() {
        collect_blue_fang_mesh_points(
            document,
            true,
            |node| node.lod_index.is_none_or(|index| index == 0),
            &mut points,
        )?;
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

pub(in crate::model_source) fn lower_netimmerse_automatic_placement_bounds(
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
    let parents = netimmerse_parents(document);
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
        collect_netimmerse_shape_points(document, &parents, seen.iter().copied(), &mut points)?;
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

/// Agents are at most this tall, so geometry above it cannot block a walk.
const FOOTPRINT_COLLISION_HULL_HEIGHT_M: f32 = 2.0;

/// Collision prism for a placeable object, recognised by its footprint
/// marker; `None` for models without one. Empty when nothing stands low
/// enough to block an agent.
pub(in crate::model_source) fn lower_blue_fang_footprint_collision_hull(
    document: &BlueFangBfbDocument,
) -> Result<Option<Vec<[f32; 3]>>> {
    if !document
        .hierarchy()
        .any(|node| node.name.eq_ignore_ascii_case("footprint"))
    {
        return Ok(None);
    }
    let nodes = document.hierarchy().collect::<Vec<_>>();
    let mut points = Vec::new();
    collect_blue_fang_mesh_points(
        document,
        // Billboards turn to face the camera; their quads have no fixed shape.
        false,
        |node| {
            node.lod_index.is_none_or(|index| index == 0)
                && !std::iter::successors(Some(node), |node| {
                    node.parent.and_then(|parent| nodes.get(parent).copied())
                })
                .take(nodes.len())
                .any(|node| node.name.eq_ignore_ascii_case("footprint"))
        },
        &mut points,
    )?;
    Ok(Some(footprint_collision_hull(&points)))
}

/// NIF counterpart of [`lower_blue_fang_footprint_collision_hull`].
pub(in crate::model_source) fn lower_netimmerse_footprint_collision_hull(
    document: &NetImmerseNifDocument,
) -> Result<Option<Vec<[f32; 3]>>> {
    let footprints = document
        .blocks()
        .filter(|block| {
            block
                .payload
                .av_object()
                .is_some_and(|object| object.object.name.eq_ignore_ascii_case("footprint"))
        })
        .map(|block| block.index.cast_signed())
        .collect::<BTreeSet<_>>();
    if footprints.is_empty() {
        return Ok(None);
    }
    let parents = netimmerse_parents(document);
    let excluded = |block_reference: i32| {
        footprints.contains(&block_reference)
            || document
                .block(block_reference)
                .and_then(|block| block.payload.av_object())
                .is_some_and(|object| object.flags & 1 != 0)
    };
    let shapes = document
        .blocks()
        .map(|block| block.index.cast_signed())
        .filter(|block_reference| {
            !std::iter::successors(Some(*block_reference), |child| parents.get(child).copied())
                .take(parents.len() + 1)
                .any(excluded)
        });
    let mut points = Vec::new();
    collect_netimmerse_shape_points(document, &parents, shapes, &mut points)?;
    Ok(Some(footprint_collision_hull(&points)))
}

/// Extrudes the outline of the geometry below agent height into a prism.
fn footprint_collision_hull(points: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let low = points
        .iter()
        .filter(|point| point[1] < FOOTPRINT_COLLISION_HULL_HEIGHT_M);
    let bottom = low
        .clone()
        .map(|point| point[1])
        .fold(f32::INFINITY, f32::min);
    let outline = convex_outline(low.map(|point| [point[0], point[2]]).collect());
    if outline.len() < 3 {
        return Vec::new();
    }
    outline
        .into_iter()
        .flat_map(|[x, z]| [[x, bottom, z], [x, FOOTPRINT_COLLISION_HULL_HEIGHT_M, z]])
        .collect()
}

/// Andrew's monotone chain; returns the hull counter-clockwise.
fn convex_outline(mut points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let half = |points: &mut dyn Iterator<Item = [f32; 2]>| {
        let mut chain: Vec<[f32; 2]> = Vec::new();
        for point in points {
            while let [.., a, b] = chain[..] {
                let turn =
                    (b[0] - a[0]).mul_add(point[1] - a[1], -(b[1] - a[1]) * (point[0] - a[0]));
                if turn > 0.0 {
                    break;
                }
                chain.pop();
            }
            chain.push(point);
        }
        chain.pop();
        chain
    };
    let mut outline = half(&mut points.iter().copied());
    outline.extend(half(&mut points.iter().rev().copied()));
    outline
}

fn collect_blue_fang_mesh_points(
    document: &BlueFangBfbDocument,
    include_billboards: bool,
    include_node: impl Fn(&BlueFangBfbNode) -> bool,
    points: &mut Vec<[f32; 3]>,
) -> Result<()> {
    for node in document.hierarchy().filter(|node| include_node(node)) {
        let mesh_block_id = match &node.kind {
            BlueFangBfbNodeKind::MeshLink { mesh_block_id, .. } => *mesh_block_id,
            BlueFangBfbNodeKind::Billboard { mesh_block_id, .. } if include_billboards => {
                *mesh_block_id
            }
            BlueFangBfbNodeKind::Billboard { .. }
            | BlueFangBfbNodeKind::Node
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
    Ok(())
}

fn netimmerse_parents(document: &NetImmerseNifDocument) -> BTreeMap<i32, i32> {
    document
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
        .collect()
}

fn collect_netimmerse_shape_points(
    document: &NetImmerseNifDocument,
    parents: &BTreeMap<i32, i32>,
    blocks: impl IntoIterator<Item = i32>,
    points: &mut Vec<[f32; 3]>,
) -> Result<()> {
    for block_reference in blocks {
        let block = document.block(block_reference).ok_or_else(|| {
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
        transform_netimmerse_points(document, parents, block_reference, &mut points[start..])?;
    }
    Ok(())
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

/// Applies a row-major BFB matrix to a source-space point.
fn transform_blue_fang_bfb_source_point(
    matrix: [[f32; 4]; 4],
    point: [f32; 3],
) -> Result<[f32; 3]> {
    if matrix.iter().flatten().any(|value| !value.is_finite()) {
        bail!("authored BFB footprint transform is invalid");
    }
    Ok([
        point[0] * matrix[0][0] + point[1] * matrix[1][0] + point[2] * matrix[2][0] + matrix[3][0],
        point[0] * matrix[0][1] + point[1] * matrix[1][1] + point[2] * matrix[2][1] + matrix[3][1],
        point[0] * matrix[0][2] + point[1] * matrix[1][2] + point[2] * matrix[2][2] + matrix[3][2],
    ])
}

fn blue_fang_bfb_point(matrix: [[f32; 4]; 4], point: [f32; 3]) -> Result<[f32; 3]> {
    transform_blue_fang_bfb_source_point(matrix, point)
        .map(convert_source_z_up_vector_to_bevy_y_up_coordinates)
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

#[cfg(test)]
mod footprint_collision_hull_tests {
    use super::footprint_collision_hull;

    #[test]
    fn hull_outlines_low_geometry_and_ignores_canopy() {
        // A 0.4 m trunk square with interior points under a 6 m wide canopy.
        let mut points = vec![
            [-0.2, 0.0, -0.2],
            [0.2, 0.0, -0.2],
            [0.2, 0.0, 0.2],
            [-0.2, 0.0, 0.2],
            [0.0, 0.0, 0.0],
            [0.1, 1.0, 0.0],
        ];
        points.extend([[-3.0, 4.0, -3.0], [3.0, 4.0, 3.0]]);
        let hull = footprint_collision_hull(&points);
        assert_eq!(hull.len(), 8);
        let near = |value: f32, expected: f32| (value - expected).abs() < 1e-6;
        assert!(hull
            .iter()
            .all(|p| near(p[0].abs(), 0.2) && near(p[2].abs(), 0.2)));
        assert!(hull.iter().all(|p| near(p[1], 0.0) || near(p[1], 2.0)));
        assert_eq!(
            footprint_collision_hull(&points[6..]),
            Vec::<[f32; 3]>::new()
        );
    }
}
