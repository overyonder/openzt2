//! Model geometry, joints and material properties.

use openzt2_game_data::AssetId;
use serde::Serialize;

#[derive(Clone, Debug)]
pub(crate) struct ModelSource {
    pub virtual_path: String,
    pub meshes: Vec<MeshSource>,
    pub skeleton: AssetId,
    pub joint_hierarchy_root: Option<JointHierarchyRootSource>,
    pub joints: Vec<JointSource>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct JointHierarchyRootSource {
    pub translation: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Clone, Debug)]
pub(crate) struct JointSource {
    pub name: String,
    pub parent: Option<u16>,
    pub local_translation: [f32; 3],
    pub local_rotation_xyzw: [f32; 4],
    pub local_scale: [f32; 3],
    pub inverse_bind: [f32; 16],
}

#[derive(Clone, Debug)]
pub(crate) struct MeshSource {
    pub vertices: Vec<VertexSource>,
    pub indices: Vec<u32>,
    pub submeshes: Vec<SubmeshSource>,
    pub lods: Vec<LodSource>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VertexSource {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uvs: [Option<[f32; 2]>; 3],
    pub uv_effects: [f32; 3],
    pub auxiliary_vector: [f32; 3],
    pub color: Option<[f32; 4]>,
    pub joints: Option<[u16; 4]>,
    pub weights: Option<[f32; 4]>,
}

#[derive(Clone, Debug)]
pub(crate) struct SubmeshSource {
    pub first_index: u32,
    pub index_count: u32,
    pub material: AssetId,
    pub native_material: Option<NativeMaterialSource>,
    pub flat_shaded: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeMaterialSource {
    pub base_color: [f32; 4],
    pub ambient: [f32; 3],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub glossiness: f32,
    pub base_texture: Option<NativeTextureSource>,
    pub detail_texture: Option<NativeTextureSource>,
    pub glow_texture: Option<NativeTextureSource>,
    pub texture_apply_mode: u32,
    pub vertex_color_mode: u32,
    pub lighting_mode: u32,
    pub alpha_blend: bool,
    pub source_blend_mode: u32,
    pub destination_blend_mode: u32,
    pub alpha_cutoff: Option<f32>,
    pub alpha_test_mode: u32,
    pub depth_test: bool,
    pub depth_write: bool,
    pub depth_test_mode: u32,
    pub cull_mode: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeTextureSource {
    pub asset_path: String,
    pub uv_set: u32,
    pub clamp_mode: u32,
    pub filter_mode: u32,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LodSource {
    pub triangle_ratio: f32,
    pub target_error: f32,
    pub screen_error: f32,
}
