//! BFB geometry, collision and hierarchy records.

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbOrientedBox {
    pub(in super::super) block_id: u32,
    pub(in super::super) name: String,
    pub(in super::super) flags: [u8; 2],
    pub(in super::super) transform: [[f32; 4]; 4],
    pub(in super::super) half_extents: [f32; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbSphere {
    pub(in super::super) block_id: u32,
    pub(in super::super) name: String,
    pub(in super::super) flags: [u8; 2],
    pub(in super::super) center: [f32; 3],
    pub(in super::super) radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbCapsule {
    pub(in super::super) block_id: u32,
    pub(in super::super) name: String,
    pub(in super::super) flags: [u8; 2],
    pub(in super::super) start: [f32; 3],
    pub(in super::super) end: [f32; 3],
    pub(in super::super) radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbMesh {
    pub(in super::super) block_id: u32,
    pub(in super::super) block_type: u16,
    pub(in super::super) name: String,
    pub(in super::super) vertex_format: String,
    pub(in super::super) stride_bytes: u32,
    pub(in super::super) primitive_kind: u8,
    pub(in super::super) vertices: Vec<BlueFangBfbVertex>,
    pub(in super::super) indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbMeshBlock {
    pub(in super::super) block_id: u32,
    pub(in super::super) block_type: u16,
    pub(in super::super) name: String,
    pub(in super::super) mesh_data_block_id: u32,
    pub(in super::super) index_start: u32,
    pub(in super::super) index_count: u32,
    pub(in super::super) vertex_start: u32,
    pub(in super::super) vertex_count: u32,
    pub(in super::super) face_count: u32,
    pub(in super::super) bounds_center: [f32; 3],
    pub(in super::super) bounds_radius: f32,
    pub(in super::super) skin: Option<BlueFangBfbSkin>,
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbSkin {
    pub(in super::super) vertex_count: u32,
    pub(in super::super) bones: Vec<BlueFangBfbBone>,
    pub(in super::super) vertex_influences: Vec<BlueFangBfbVertexInfluence>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in super::super) struct BlueFangBfbVertexInfluence {
    pub(in super::super) joint_indices: [u16; 4],
    pub(in super::super) joint_weights: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbBone {
    pub(in super::super) identity: [u8; 3],
    pub(in super::super) name: String,
    pub(in super::super) transform: [[f32; 4]; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbNode {
    pub(in super::super) block_id: u32,
    pub(in super::super) name: String,
    pub(in super::super) parent: Option<usize>,
    pub(in super::super) lod_index: Option<u32>,
    pub(in super::super) local_transform: [[f32; 4]; 4],
    pub(in super::super) world_transform: [[f32; 4]; 4],
    pub(in super::super) kind: BlueFangBfbNodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in super::super) enum BlueFangBfbNodeKind {
    Node,
    LodGroup,
    MeshLink {
        mesh_block_id: u32,
        material_name: String,
    },
    Billboard {
        mesh_block_id: u32,
        material_name: String,
        mode: u32,
    },
    ParticleSystem {
        resource_name: String,
    },
    ModelJoint {
        component_name: String,
    },
    Other(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct BlueFangBfbVertex {
    pub(in super::super) position: [f32; 3],
    pub(in super::super) normal: Option<[f32; 3]>,
    pub(in super::super) diffuse: Option<u32>,
    pub(in super::super) tex0: Option<[f32; 2]>,
    /// Authored BFR `Tn`/`T3n` streams. XY are sampled texture coordinates;
    /// Z is the effect value carried by three-component declarations.
    pub(in super::super) texture_coordinates: Vec<[f32; 3]>,
    /// First authored three-component auxiliary declaration (`T3D1`/`T3D2`).
    /// These streams carry renderer effect vectors rather than texture UVs.
    pub(in super::super) auxiliary_vector: Option<[f32; 3]>,
}
