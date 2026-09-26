//! Source-faithful NetImmerse animation controllers, sequences, morphs, and text keys.

use super::interpolated_key_source_types::{
    NetImmerseFloatKey, NetImmerseFloatKeyGroup, NetImmerseQuaternionKey, NetImmerseVector3KeyGroup,
};

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiTimeController {
    pub(in super::super) next_controller_ref: i32,
    pub(in super::super) flags: u16,
    pub(in super::super) frequency: f32,
    pub(in super::super) phase: f32,
    pub(in super::super) start_time: f32,
    pub(in super::super) stop_time: f32,
    pub(in super::super) target_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiAlphaController {
    pub(in super::super) controller: NetImmerseNiTimeController,
    pub(in super::super) data_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiKeyframeController {
    pub(in super::super) controller: NetImmerseNiTimeController,
    pub(in super::super) data_ref: i32,
}
/// Root animation sequence used by Zoo Tycoon 2's NetImmerse 10.0.1.0 KF
/// files. Later Gamebryo versions add interpolator/string-palette fields; the
/// versions accepted by this frontend use direct target names and controller
/// references.
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiControllerSequence {
    pub(in super::super) name: String,
    pub(in super::super) accum_root_name: String,
    pub(in super::super) text_keys_ref: i32,
    pub(in super::super) controlled_blocks: Vec<NetImmerseControlledBlock>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseControlledBlock {
    pub(in super::super) target_name: String,
    pub(in super::super) controller_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiMaterialColorController {
    pub(in super::super) controller: NetImmerseNiTimeController,
    pub(in super::super) target_color: Option<u32>,
    pub(in super::super) data_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiUvController {
    pub(in super::super) controller: NetImmerseNiTimeController,
    pub(in super::super) texture_set: u16,
    pub(in super::super) data_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiGeomMorpherController {
    pub(in super::super) controller: NetImmerseNiTimeController,
    pub(in super::super) morpher_flags: Option<u16>,
    pub(in super::super) data_ref: i32,
    pub(in super::super) always_update: Option<u8>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiVisController {
    pub(in super::super) controller: NetImmerseNiTimeController,
    pub(in super::super) data_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiKeyframeData {
    pub(in super::super) num_rotation_keys: u32,
    pub(in super::super) rotation_type: Option<u32>,
    pub(in super::super) quaternion_keys: Vec<NetImmerseQuaternionKey>,
    pub(in super::super) order: Option<f32>,
    pub(in super::super) xyz_rotations: Vec<NetImmerseFloatKeyGroup>,
    pub(in super::super) translations: NetImmerseVector3KeyGroup,
    pub(in super::super) scales: NetImmerseFloatKeyGroup,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiMorphData {
    pub(in super::super) num_morphs: u32,
    pub(in super::super) num_vertices: u32,
    pub(in super::super) relative_targets: u8,
    pub(in super::super) morphs: Vec<NetImmerseMorph>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseMorph {
    pub(in super::super) num_keys: u32,
    pub(in super::super) interpolation: u32,
    pub(in super::super) keys: Vec<NetImmerseFloatKey>,
    pub(in super::super) vectors: Vec<[f32; 3]>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiTextKeyExtraData {
    pub(in super::super) name: String,
    pub(in super::super) keys: Vec<NetImmerseTextKey>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseTextKey {
    pub(in super::super) time: f32,
    pub(in super::super) value: String,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiBoneLodController {
    pub(in super::super) controller: NetImmerseNiTimeController,
    pub(in super::super) lod: u32,
    pub(in super::super) node_groups: Vec<Vec<i32>>,
    pub(in super::super) shape_groups: Vec<Vec<NetImmerseNiBoneLodSkinInfo>>,
    pub(in super::super) shape_refs: Vec<i32>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiBoneLodSkinInfo {
    pub(in super::super) shape_ref: i32,
    pub(in super::super) skin_instance_ref: i32,
}
