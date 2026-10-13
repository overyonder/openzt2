//! Source-faithful NetImmerse interpolated key groups and sampled data records.

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiColorData {
    pub(in super::super) data: NetImmerseColorKeyGroup,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiFloatData {
    pub(in super::super) data: NetImmerseFloatKeyGroup,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiPosData {
    pub(in super::super) data: NetImmerseVector3KeyGroup,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiUvData {
    pub(in super::super) groups: Vec<NetImmerseFloatKeyGroup>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiVisData {
    pub(in super::super) keys: Vec<NetImmerseByteKey>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseColorKeyGroup {
    pub(in super::super) interpolation: Option<u32>,
    pub(in super::super) keys: Vec<NetImmerseColorKey>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseColorKey {
    pub(in super::super) time: f32,
    pub(in super::super) value: [f32; 4],
    pub(in super::super) forward: Option<[f32; 4]>,
    pub(in super::super) backward: Option<[f32; 4]>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseFloatKeyGroup {
    pub(in super::super) interpolation: Option<u32>,
    pub(in super::super) keys: Vec<NetImmerseFloatKey>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseFloatKey {
    pub(in super::super) time: f32,
    pub(in super::super) value: f32,
    pub(in super::super) forward: Option<f32>,
    pub(in super::super) backward: Option<f32>,
    pub(in super::super) tbc: Option<[f32; 3]>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseByteKey {
    pub(in super::super) time: f32,
    pub(in super::super) value: u8,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseQuaternionKey {
    pub(in super::super) time: f32,
    pub(in super::super) value: [f32; 4],
    pub(in super::super) forward: Option<[f32; 4]>,
    pub(in super::super) backward: Option<[f32; 4]>,
    pub(in super::super) tbc: Option<[f32; 3]>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseVector3KeyGroup {
    pub(in super::super) interpolation: Option<u32>,
    pub(in super::super) keys: Vec<NetImmerseVector3Key>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseVector3Key {
    pub(in super::super) time: f32,
    pub(in super::super) value: [f32; 3],
    pub(in super::super) forward: Option<[f32; 3]>,
    pub(in super::super) backward: Option<[f32; 3]>,
    pub(in super::super) tbc: Option<[f32; 3]>,
}
