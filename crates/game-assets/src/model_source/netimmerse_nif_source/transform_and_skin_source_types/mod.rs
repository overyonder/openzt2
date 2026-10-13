//! Source-faithful NetImmerse transform, skin, influence, and partition records.

use super::collision_source_types::NetImmerseNiBound;

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiTransform {
    pub(in super::super) rotation: [f32; 9],
    pub(in super::super) translation: [f32; 3],
    pub(in super::super) scale: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiSkinData {
    pub(in super::super) skin_transform: NetImmerseNiTransform,
    pub(in super::super) skin_partition_ref: Option<i32>,
    pub(in super::super) has_vertex_weights: bool,
    pub(in super::super) bones: Vec<NetImmerseNiSkinBoneData>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiSkinBoneData {
    pub(in super::super) skin_transform: NetImmerseNiTransform,
    pub(in super::super) bounding_sphere: NetImmerseNiBound,
    pub(in super::super) vertex_weights: Vec<NetImmerseNiSkinWeight>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiSkinWeight {
    pub(in super::super) vertex_index: u16,
    pub(in super::super) weight: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiSkinInstance {
    pub(in super::super) data_ref: i32,
    pub(in super::super) skin_partition_ref: Option<i32>,
    pub(in super::super) skeleton_root_ref: i32,
    pub(in super::super) bone_refs: Vec<i32>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiSkinPartition {
    pub(in super::super) partitions: Vec<NetImmerseSkinPartition>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseSkinPartition {
    pub(in super::super) vertex_count: u16,
    pub(in super::super) triangle_count: u16,
    pub(in super::super) bone_count: u16,
    pub(in super::super) strip_count: u16,
    pub(in super::super) weights_per_vertex: u16,
    pub(in super::super) bones: Vec<u16>,
    pub(in super::super) vertex_map: Vec<u16>,
    pub(in super::super) vertex_weights: Vec<Vec<f32>>,
    pub(in super::super) strip_lengths: Vec<u16>,
    pub(in super::super) strips: Vec<Vec<u16>>,
    pub(in super::super) triangles: Vec<[u16; 3]>,
    pub(in super::super) bone_indices: Option<Vec<Vec<u8>>>,
}
