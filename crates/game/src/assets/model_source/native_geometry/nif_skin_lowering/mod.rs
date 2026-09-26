//! `NetImmerse` skeleton ordering, bind transforms, and vertex-weight lowering.

use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

use super::super::{
    model::{JointHierarchyRootSource, JointSource, VertexSource},
    native_geometry_lowering_error::{
        NativeGeometryLoweringError, NativeGeometryLoweringErrorKind, NativeGeometrySourceFamily,
    },
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
    },
};
use super::{
    nif_geometry_lowering::missing_netimmerse_geometry,
    source_transform_conversion::{
        convert_netimmerse_transform_to_gltf_matrix, convert_source_transform_to_bevy_components,
    },
    validated_vertex_topology_assembly::native_geometry_lowering_error,
};

pub(super) fn lower_netimmerse_skin(
    document: &NetImmerseNifDocument,
    instance_ref: i32,
    vertices: &mut [VertexSource],
    geometry_block: u32,
) -> Result<
    (AssetId, Option<JointHierarchyRootSource>, Vec<JointSource>),
    NativeGeometryLoweringError,
> {
    let instance = document
        .block(instance_ref)
        .and_then(|block| match &block.payload {
            NetImmerseNifBlockPayload::NiSkinInstance(instance) => Some(instance),
            _ => None,
        })
        .ok_or_else(|| missing_netimmerse_geometry(document, geometry_block))?;
    let data = document
        .block(instance.data_ref)
        .and_then(|block| match &block.payload {
            NetImmerseNifBlockPayload::NiSkinData(data) => Some(data),
            _ => None,
        })
        .ok_or_else(|| missing_netimmerse_geometry(document, geometry_block))?;
    if data.bones.len() != instance.bone_refs.len() {
        return Err(native_geometry_lowering_error(
            NativeGeometrySourceFamily::Nif,
            &document.source_path,
            NativeGeometryLoweringErrorKind::SkeletonLayoutMissing {
                geometry: format!("NIF geometry block {geometry_block}"),
            },
        ));
    }
    let skeleton_error = |detail: &str| {
        native_geometry_lowering_error(
            NativeGeometrySourceFamily::Nif,
            &document.source_path,
            NativeGeometryLoweringErrorKind::SkeletonLayoutMissing {
                geometry: detail.to_owned(),
            },
        )
    };
    let mut parents = BTreeMap::new();
    for block in document.blocks() {
        let parent = i32::try_from(block.index)
            .map_err(|_| skeleton_error("NIF parent index exceeds source reference range"))?;
        for child in block.payload.child_and_effect_refs().0 {
            if *child >= 0 && parents.insert(*child, parent).is_some() {
                return Err(skeleton_error("NIF scene node has multiple parents"));
            }
        }
    }
    // Native deformation uses the bone's complete world transform relative to
    // the skeleton root parent. Retain named intermediary nodes so animation
    // still replaces their original local transforms, rather than baking them
    // into a descendant or silently treating that descendant as a root.
    let order = order_netimmerse_skin_joint_hierarchy(
        instance.skeleton_root_ref,
        &instance.bone_refs,
        &parents,
    )
    .map_err(skeleton_error)?;
    let node_indices = order
        .iter()
        .enumerate()
        .map(|(index, reference)| {
            u16::try_from(index)
                .map(|index| (*reference, index))
                .map_err(|_| skeleton_error("NIF skeleton exceeds glTF joint index range"))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let remap = instance
        .bone_refs
        .iter()
        .enumerate()
        .map(|(old, reference)| (old, node_indices[reference]))
        .collect::<BTreeMap<_, _>>();
    let joints = order
        .iter()
        .map(|reference| {
            let object = document
                .block(*reference)
                .and_then(|block| block.payload.av_object())
                .ok_or_else(|| {
                    skeleton_error("NIF skeleton references a missing transform node")
                })?;
            let (local_translation, local_rotation_xyzw, local_scale) =
                convert_source_transform_to_bevy_components(
                    object.translation,
                    object.rotation,
                    object.scale,
                );
            let parent = parents
                .get(reference)
                .and_then(|parent| node_indices.get(parent))
                .copied();
            let inverse_bind = instance
                .bone_refs
                .iter()
                .position(|bone| bone == reference)
                .map_or(bevy::math::Mat4::IDENTITY.to_cols_array(), |old| {
                    convert_netimmerse_transform_to_gltf_matrix(&data.bones[old].skin_transform)
                });
            Ok(JointSource {
                name: object.object.name.clone(),
                parent,
                local_translation,
                local_rotation_xyzw,
                local_scale,
                inverse_bind,
            })
        })
        .collect::<Result<Vec<_>, NativeGeometryLoweringError>>()?;

    let mut influences = vec![Vec::<(u16, f32)>::new(); vertices.len()];
    data.bones.iter().enumerate().for_each(|(old, bone)| {
        bone.vertex_weights.iter().for_each(|weight| {
            if weight.weight.is_finite() && weight.weight > 0.0 {
                if let Some(vertex) = influences.get_mut(usize::from(weight.vertex_index)) {
                    vertex.push((remap[&old], weight.weight));
                }
            }
        });
    });
    let partition_ref = instance.skin_partition_ref.or(data.skin_partition_ref);
    if let Some(partition) = partition_ref.and_then(|reference| {
        document
            .block(reference)
            .and_then(|block| match &block.payload {
                NetImmerseNifBlockPayload::NiSkinPartition(partition) => Some(partition),
                _ => None,
            })
    }) {
        for partition in &partition.partitions {
            let Some(indices) = partition.bone_indices.as_ref() else {
                continue;
            };
            for (row, source_vertex) in partition.vertex_map.iter().enumerate() {
                let Some(vertex) = influences.get_mut(usize::from(*source_vertex)) else {
                    continue;
                };
                if !vertex.is_empty() {
                    continue;
                }
                let Some((bone_indices, weights)) =
                    indices.get(row).zip(partition.vertex_weights.get(row))
                else {
                    continue;
                };
                bone_indices
                    .iter()
                    .zip(weights)
                    .for_each(|(palette, weight)| {
                        let old = partition
                            .bones
                            .get(usize::from(*palette))
                            .copied()
                            .map(usize::from);
                        if weight.is_finite() && *weight > 0.0 {
                            if let Some(joint) = old.and_then(|old| remap.get(&old)).copied() {
                                vertex.push((joint, *weight));
                            }
                        }
                    });
            }
        }
    }
    vertices
        .iter_mut()
        .zip(influences)
        .for_each(|(vertex, mut influences)| {
            influences.sort_by(|left, right| right.1.total_cmp(&left.1));
            influences.truncate(4);
            if influences.is_empty() {
                influences.push((0, 1.0));
            }
            let sum = influences.iter().map(|(_, weight)| weight).sum::<f32>();
            let mut indices = [0; 4];
            let mut weights = [0.0; 4];
            influences
                .iter()
                .enumerate()
                .for_each(|(slot, (joint, weight))| {
                    indices[slot] = *joint;
                    weights[slot] = *weight / sum;
                });
            vertex.joints = Some(indices);
            vertex.weights = Some(weights);
        });
    let (translation, rotation_xyzw, scale) = convert_source_transform_to_bevy_components(
        data.skin_transform.translation,
        data.skin_transform.rotation,
        data.skin_transform.scale,
    );
    Ok((
        AssetId::from_key(&format!(
            "{}.__skeleton/nif_{instance_ref:08x}",
            document.source_path.as_str()
        )),
        Some(JointHierarchyRootSource {
            translation,
            rotation_xyzw,
            scale,
        }),
        joints,
    ))
}

fn order_netimmerse_skin_joint_hierarchy(
    root: i32,
    bones: &[i32],
    parents: &BTreeMap<i32, i32>,
) -> Result<Vec<i32>, &'static str> {
    if root < 0 {
        return Err("NIF skin has no skeleton root");
    }
    let mut order = vec![root];
    let mut branch = Vec::new();
    for bone in bones {
        branch.clear();
        let mut node = *bone;
        while !order.contains(&node) {
            if branch.contains(&node) {
                return Err("cyclic NIF skeleton");
            }
            branch.push(node);
            node = *parents
                .get(&node)
                .ok_or("NIF bone is outside its skeleton root")?;
        }
        order.extend(branch.iter().rev().copied());
    }
    if parents
        .get(&root)
        .is_some_and(|parent| order.contains(parent))
    {
        return Err("cyclic NIF skeleton root");
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_hierarchy_retains_unweighted_animation_nodes_and_orders_parents_first() {
        let parents = BTreeMap::from([(11, 10), (12, 11), (13, 12), (14, 11)]);
        assert_eq!(
            order_netimmerse_skin_joint_hierarchy(10, &[13, 14, 12], &parents),
            Ok(vec![10, 11, 12, 13, 14]),
        );
    }

    #[test]
    fn skin_hierarchy_rejects_disconnected_and_cyclic_bones() {
        assert!(order_netimmerse_skin_joint_hierarchy(10, &[12], &BTreeMap::new()).is_err());
        let parents = BTreeMap::from([(11, 12), (12, 11)]);
        assert!(order_netimmerse_skin_joint_hierarchy(10, &[12], &parents).is_err());
        let parents = BTreeMap::from([(10, 12), (12, 10)]);
        assert!(order_netimmerse_skin_joint_hierarchy(10, &[12], &parents).is_err());
    }
}
