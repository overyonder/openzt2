//! GLB encoding for model geometry.

use std::collections::BTreeMap;

use gltf::json::{
    accessor::Type,
    buffer::Target,
    material::{Material, PbrMetallicRoughness, StrengthFactor},
    mesh::{Mesh, Mode, Primitive, Semantic},
    scene::{Node, Scene, UnitQuaternion},
    skin::Skin,
    validation::Checked,
    Extras, Index, Root,
};
use openzt2_game_data::AssetId;
use serde::Serialize;

use super::{
    conversion_error::ConversionError,
    gltf_binary_buffer::GltfBinaryBuffer,
    model::{MeshSource, ModelSource, SubmeshSource, VertexSource},
};

pub(super) fn encode_models_as_glb(models: Vec<ModelSource>) -> Result<Vec<u8>, ConversionError> {
    NativeModelGltfWriter::default().write(models)
}

#[derive(Default)]
struct NativeModelGltfWriter {
    root: Root,
    binary_buffer: GltfBinaryBuffer,
    material_indices: BTreeMap<AssetId, Index<Material>>,
}

impl NativeModelGltfWriter {
    fn write(mut self, models: Vec<ModelSource>) -> Result<Vec<u8>, ConversionError> {
        let model_roots = models
            .into_iter()
            .map(|model| {
                let scene_name = model.virtual_path.clone();
                self.add_model(model)
                    .map(|root_node| (scene_name, root_node))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !model_roots.is_empty() {
            self.root.scenes = model_roots
                .into_iter()
                .map(|(scene_name, root_node)| Scene {
                    extensions: None,
                    extras: None,
                    name: Some(scene_name),
                    nodes: vec![root_node],
                })
                .collect();
        }
        // Particle-only NIFs own their presentation through the particle
        // subassets. They have no mesh scene to select in the glTF carrier.
        self.root.scene = (!self.root.scenes.is_empty()).then(|| Index::new(0));
        self.binary_buffer.finish(self.root)
    }

    fn add_model(&mut self, model: ModelSource) -> Result<Index<Node>, ConversionError> {
        let mesh_indices = model
            .meshes
            .iter()
            .map(|mesh| self.add_mesh(mesh))
            .collect::<Result<Vec<_>, _>>()?;
        mesh_indices.iter().for_each(|mesh_index| {
            self.root.meshes[mesh_index.value()].name = Some(model.virtual_path.clone());
        });
        let (joint_hierarchy_root_nodes, skin) = self.add_skin(&model)?;
        let root_node = self.root.push(Node {
            children: Some(joint_hierarchy_root_nodes),
            extras: (model.skeleton != AssetId::default())
                .then(|| {
                    gltf_extras(&serde_json::json!({
                        "openzt2Skeleton": model.skeleton.to_lowercase_hexadecimal_string(),
                    }))
                })
                .transpose()?
                .flatten(),
            mesh: mesh_indices.first().copied(),
            name: Some(model.virtual_path),
            skin,
            ..Default::default()
        });
        for mesh in mesh_indices.into_iter().skip(1) {
            let child_node = self.root.push(Node {
                mesh: Some(mesh),
                skin,
                ..Default::default()
            });
            self.root.nodes[root_node.value()]
                .children
                .get_or_insert_default()
                .push(child_node);
        }
        Ok(root_node)
    }

    fn add_skin(
        &mut self,
        model: &ModelSource,
    ) -> Result<(Vec<Index<Node>>, Option<Index<Skin>>), ConversionError> {
        if model.joints.is_empty() {
            return Ok((Vec::new(), None));
        }
        let first_joint_node = self.root.nodes.len();
        let joint_node_indices = (0..model.joints.len())
            .map(|joint_index| {
                first_joint_node
                    .checked_add(joint_index)
                    .and_then(|node_index| u32::try_from(node_index).ok())
                    .map(Index::new)
                    .ok_or(ConversionError::InvalidValue(
                        "model joint node index exceeds glTF limits",
                    ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        model
            .joints
            .iter()
            .enumerate()
            .for_each(|(joint_index, joint)| {
                let children = model
                    .joints
                    .iter()
                    .enumerate()
                    .filter_map(|(child_index, child)| {
                        (child.parent == u16::try_from(joint_index).ok())
                            .then_some(joint_node_indices[child_index])
                    })
                    .collect::<Vec<_>>();
                self.root.nodes.push(Node {
                    children: (!children.is_empty()).then_some(children),
                    name: Some(joint.name.clone()),
                    rotation: Some(UnitQuaternion(joint.local_rotation_xyzw)),
                    scale: Some(joint.local_scale),
                    translation: Some(joint.local_translation),
                    ..Default::default()
                });
            });
        let inverse_bind_matrices = model
            .joints
            .iter()
            .flat_map(|joint| joint.inverse_bind)
            .collect::<Vec<_>>();
        let inverse_bind_accessor = self.binary_buffer.add_f32_accessor(
            &mut self.root,
            &inverse_bind_matrices,
            Type::Mat4,
            false,
            None,
        );
        let root_joint_nodes = model
            .joints
            .iter()
            .enumerate()
            .filter_map(|(joint_index, joint)| {
                joint
                    .parent
                    .is_none()
                    .then_some(joint_node_indices[joint_index])
            })
            .collect::<Vec<_>>();
        let (joint_hierarchy_root_nodes, skeleton) = match model.joint_hierarchy_root {
            None => (root_joint_nodes, None),
            Some(joint_hierarchy_root) => {
                let joint_hierarchy_root_node = self.root.push(Node {
                    children: Some(root_joint_nodes),
                    rotation: Some(UnitQuaternion(joint_hierarchy_root.rotation_xyzw)),
                    scale: Some(joint_hierarchy_root.scale),
                    translation: Some(joint_hierarchy_root.translation),
                    ..Default::default()
                });
                (
                    vec![joint_hierarchy_root_node],
                    Some(joint_hierarchy_root_node),
                )
            }
        };
        let skin = self.root.push(Skin {
            extensions: None,
            extras: None,
            inverse_bind_matrices: Some(inverse_bind_accessor),
            joints: joint_node_indices.clone(),
            name: None,
            skeleton,
        });
        Ok((joint_hierarchy_root_nodes, Some(skin)))
    }

    fn add_mesh(&mut self, mesh: &MeshSource) -> Result<Index<Mesh>, ConversionError> {
        if mesh.vertices.is_empty() {
            return Err(ConversionError::InvalidValue("model mesh has no vertices"));
        }
        let mut attributes = BTreeMap::from_iter([
            (
                Checked::Valid(Semantic::Positions),
                self.add_vertex_f32_accessor(&mesh.vertices, Type::Vec3, true, |vertex| {
                    vertex.position
                }),
            ),
            (
                Checked::Valid(Semantic::Normals),
                self.add_vertex_f32_accessor(&mesh.vertices, Type::Vec3, false, |vertex| {
                    vertex.normal
                }),
            ),
            (
                Checked::Valid(Semantic::Tangents),
                self.add_vertex_f32_accessor(&mesh.vertices, Type::Vec4, false, |vertex| {
                    vertex.tangent
                }),
            ),
            (
                Checked::Valid(Semantic::Extras("_UV_EFFECTS".to_owned())),
                self.add_vertex_f32_accessor(&mesh.vertices, Type::Vec3, false, |vertex| {
                    vertex.uv_effects
                }),
            ),
            (
                Checked::Valid(Semantic::Extras("_AUXILIARY_VECTOR".to_owned())),
                self.add_vertex_f32_accessor(&mesh.vertices, Type::Vec3, false, |vertex| {
                    vertex.auxiliary_vector
                }),
            ),
        ]);
        for texture_coordinate_set in 0..3 {
            if texture_coordinate_set < 2
                || mesh
                    .vertices
                    .iter()
                    .any(|vertex| vertex.uvs[texture_coordinate_set].is_some())
            {
                let accessor =
                    self.add_vertex_f32_accessor(&mesh.vertices, Type::Vec2, false, |vertex| {
                        vertex.uvs[texture_coordinate_set]
                            .or(vertex.uvs[0])
                            .unwrap_or_default()
                    });
                let semantic = if texture_coordinate_set == 2 {
                    Semantic::Extras("_UV_2".to_owned())
                } else {
                    Semantic::TexCoords(
                        u32::try_from(texture_coordinate_set)
                            .expect("three texture-coordinate sets fit u32"),
                    )
                };
                attributes.insert(Checked::Valid(semantic), accessor);
            }
        }
        let vertex_colours =
            self.add_vertex_f32_accessor(&mesh.vertices, Type::Vec4, false, |vertex| {
                vertex.color.unwrap_or([1.0; 4])
            });
        attributes.insert(Checked::Valid(Semantic::Colors(0)), vertex_colours);
        let has_joints = mesh.vertices.iter().all(|vertex| vertex.joints.is_some());
        let has_weights = mesh.vertices.iter().all(|vertex| vertex.weights.is_some());
        if has_joints != has_weights {
            return Err(ConversionError::InvalidValue(
                "model vertices have incomplete skin bindings",
            ));
        }
        if has_joints {
            let joint_values = mesh
                .vertices
                .iter()
                .flat_map(|vertex| vertex.joints.unwrap_or_default())
                .collect::<Vec<_>>();
            let joints = self.binary_buffer.add_u16_accessor(
                &mut self.root,
                &joint_values,
                Type::Vec4,
                Some(Target::ArrayBuffer),
            );
            let weights =
                self.add_vertex_f32_accessor(&mesh.vertices, Type::Vec4, false, |vertex| {
                    vertex.weights.unwrap_or_default()
                });
            attributes.insert(Checked::Valid(Semantic::Joints(0)), joints);
            attributes.insert(Checked::Valid(Semantic::Weights(0)), weights);
        }
        let primitives = mesh
            .submeshes
            .iter()
            .map(|submesh| self.add_submesh_primitive(mesh, submesh, attributes.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        let mesh_index = self.root.push(Mesh {
            extensions: None,
            extras: gltf_extras(&serde_json::json!({ "openzt2Lods": mesh.lods }))?,
            name: None,
            primitives,
            weights: None,
        });
        Ok(mesh_index)
    }

    fn add_submesh_primitive(
        &mut self,
        mesh: &MeshSource,
        submesh: &SubmeshSource,
        attributes: BTreeMap<Checked<Semantic>, Index<gltf::json::accessor::Accessor>>,
    ) -> Result<Primitive, ConversionError> {
        let start = usize::try_from(submesh.first_index)
            .map_err(|_| ConversionError::InvalidValue("submesh index exceeds usize"))?;
        let count = usize::try_from(submesh.index_count)
            .map_err(|_| ConversionError::InvalidValue("submesh count exceeds usize"))?;
        let end = start
            .checked_add(count)
            .filter(|end| *end <= mesh.indices.len())
            .ok_or(ConversionError::InvalidValue(
                "submesh range exceeds index buffer",
            ))?;
        let indices = self.binary_buffer.add_u32_accessor(
            &mut self.root,
            &mesh.indices[start..end],
            Type::Scalar,
            Some(Target::ElementArrayBuffer),
        );
        let material = self.add_material(submesh.material)?;
        Ok(Primitive {
            attributes,
            extensions: None,
            extras: gltf_extras(&serde_json::json!({ "flatShaded": submesh.flat_shaded }))?,
            indices: Some(indices),
            material: Some(material),
            mode: Checked::Valid(Mode::Triangles),
            targets: None,
        })
    }

    fn add_material(&mut self, material_id: AssetId) -> Result<Index<Material>, ConversionError> {
        if let Some(index) = self.material_indices.get(&material_id) {
            return Ok(*index);
        }
        let material = Material {
            name: Some(material_id.to_lowercase_hexadecimal_string()),
            pbr_metallic_roughness: PbrMetallicRoughness {
                metallic_factor: StrengthFactor(0.0),
                roughness_factor: StrengthFactor(1.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let material_index = self.root.push(material);
        self.material_indices.insert(material_id, material_index);
        Ok(material_index)
    }

    fn add_vertex_f32_accessor<const COMPONENTS: usize>(
        &mut self,
        vertices: &[VertexSource],
        accessor_type: Type,
        include_bounds: bool,
        value: impl Fn(&VertexSource) -> [f32; COMPONENTS],
    ) -> Index<gltf::json::accessor::Accessor> {
        self.binary_buffer.add_f32_accessor(
            &mut self.root,
            &vertices.iter().flat_map(value).collect::<Vec<_>>(),
            accessor_type,
            include_bounds,
            Some(Target::ArrayBuffer),
        )
    }
}

fn gltf_extras(value: &impl Serialize) -> Result<Extras, ConversionError> {
    serde_json::value::to_raw_value(value)
        .map(Some)
        .map_err(Into::into)
}
