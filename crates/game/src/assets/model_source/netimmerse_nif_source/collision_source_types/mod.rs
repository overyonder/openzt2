//! Source-faithful NetImmerse collision-object and bounding-volume records.

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiCollisionObject {
    pub(in super::super) target_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiCollisionData {
    pub(in super::super) object: NetImmerseNiCollisionObject,
    pub(in super::super) propagation_mode: u32,
    pub(in super::super) collision_mode: Option<u32>,
    pub(in super::super) use_abv: u8,
    pub(in super::super) bounding_volume: Option<NetImmerseBoundingVolume>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) enum NetImmerseBoundingVolume {
    Sphere(NetImmerseNiBound),
    Box(NetImmerseBoxBoundingVolume),
    Capsule(NetImmerseCapsuleBoundingVolume),
    Union(Vec<NetImmerseBoundingVolume>),
    HalfSpace(NetImmerseHalfSpaceBoundingVolume),
    Unknown(u32),
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiBound {
    pub(in super::super) center: [f32; 3],
    pub(in super::super) radius: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseBoxBoundingVolume {
    pub(in super::super) center: [f32; 3],
    pub(in super::super) axes: Vec<[f32; 3]>,
    pub(in super::super) extent: [f32; 3],
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseCapsuleBoundingVolume {
    pub(in super::super) center: [f32; 3],
    pub(in super::super) origin: [f32; 3],
    pub(in super::super) extent: f32,
    pub(in super::super) radius: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseHalfSpaceBoundingVolume {
    pub(in super::super) plane: NetImmerseNiPlane,
    pub(in super::super) center: [f32; 3],
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiPlane {
    pub(in super::super) normal: [f32; 3],
    pub(in super::super) constant: f32,
}
