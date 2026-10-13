//! Source-faithful NetImmerse scene graph, light, and geometry-object records.

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiObjectNet {
    pub(in super::super) name: String,
    pub(in super::super) extra_data_refs: Vec<i32>,
    pub(in super::super) controller_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiAvObject {
    pub(in super::super) object: NetImmerseNiObjectNet,
    pub(in super::super) flags: u16,
    pub(in super::super) translation: [f32; 3],
    pub(in super::super) rotation: [f32; 9],
    pub(in super::super) scale: f32,
    pub(in super::super) property_refs: Vec<i32>,
    pub(in super::super) collision_object_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiLight {
    pub(in super::super) av_object: NetImmerseNiAvObject,
    pub(in super::super) dimmer: f32,
    pub(in super::super) ambient: [f32; 3],
    pub(in super::super) diffuse: [f32; 3],
    pub(in super::super) specular: [f32; 3],
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiDirectionalLight {
    pub(in super::super) light: NetImmerseNiLight,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiAmbientLight {
    pub(in super::super) light: NetImmerseNiLight,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiPointLight {
    pub(in super::super) light: NetImmerseNiLight,
    pub(in super::super) constant_attenuation: f32,
    pub(in super::super) linear_attenuation: f32,
    pub(in super::super) quadratic_attenuation: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiNode {
    pub(in super::super) av_object: NetImmerseNiAvObject,
    pub(in super::super) child_refs: Vec<i32>,
    pub(in super::super) effect_refs: Vec<i32>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiBillboardNode {
    pub(in super::super) node: NetImmerseNiNode,
    pub(in super::super) billboard_mode: Option<u32>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiSwitchNode {
    pub(in super::super) node: NetImmerseNiNode,
    pub(in super::super) switch_flags: Option<u16>,
    pub(in super::super) active_child_index: u32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiLodNode {
    pub(in super::super) switch_node: NetImmerseNiSwitchNode,
    pub(in super::super) lod_center: Option<[f32; 3]>,
    pub(in super::super) lod_levels: Vec<NetImmerseLodRange>,
    pub(in super::super) lod_level_data_ref: Option<i32>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseLodRange {
    pub(in super::super) near_extent: f32,
    pub(in super::super) far_extent: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiGeometry {
    pub(in super::super) av_object: NetImmerseNiAvObject,
    pub(in super::super) data_ref: i32,
    pub(in super::super) skin_instance_ref: i32,
    pub(in super::super) shader: Option<NetImmerseNiGeometryShader>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiGeometryShader {
    pub(in super::super) name: String,
    pub(in super::super) extra_data: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticles {
    pub(in super::super) geometry: NetImmerseNiGeometry,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiTriStrips {
    pub(in super::super) geometry: NetImmerseNiGeometry,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiTriShape {
    pub(in super::super) geometry: NetImmerseNiGeometry,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleMeshes {
    pub(in super::super) geometry: NetImmerseNiGeometry,
}
