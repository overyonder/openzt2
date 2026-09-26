//! Immutable scene-prefab documents with stable entity indexes.

mod scene_prefab_flag_operations;

use crate::AssetId;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct ScenePrefabAssetDependency {
    pub asset_id: AssetId,
    pub asset_path: String,
    pub asset_kind: ScenePrefabAssetDependencyKind,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub enum ScenePrefabAssetDependencyKind {
    Model,
    Material,
    Effect,
    Animation,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct ScenePrefabDocument {
    pub entities: Vec<PrefabEntity>,
    /// Source-lowered horizontal bounds used by authored
    /// `ZTPlacementData autoFootprint`. The first row is the minimum X/Z and
    /// the second is the maximum X/Z.
    pub automatic_placement_bounds_xz: Option<[[f32; 2]; 2]>,
    /// Optional authored presentation camera carried by scene documents such
    /// as the credits rail. Static objects remain ordinary prefab entities;
    /// only the camera/environment timeline lives in this focused record.
    pub rail_camera: Option<PrefabRailCamera>,
    pub dependencies: Vec<ScenePrefabAssetDependency>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct ScenePrefabEntityFlags(pub u8);

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct PrefabEntity {
    pub stable_id: AssetId,
    /// Prefab-local authored attachment identity. Unlike `stable_id`, this is
    /// identical across prefab variants which expose the same named socket.
    pub attachment_id: AssetId,
    /// Named model joint whose world transform owns this attachment.
    #[serde(default)]
    pub model_joint_binding: Option<String>,
    pub parent: u32,
    pub transform: PrefabTransform,
    pub children: Vec<u32>,
    pub flags: ScenePrefabEntityFlags,
    pub rotation_cycles: Vec<PrefabRotationCycle>,
    #[serde(default)]
    pub transform_animations: Vec<PrefabTransformAnimation>,
    pub renderables: Vec<PrefabRenderable>,
    pub colliders: Vec<PrefabCollider>,
    /// Distance ranges for renderables grouped under this authored entity.
    pub lods: Vec<PrefabLod>,
    pub billboards: Vec<PrefabBillboard>,
    pub lights: Vec<PrefabLight>,
    pub effects: Vec<PrefabEffect>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabTransform {
    pub translation_m: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub scale: [f32; 3],
}

/// An authored, continuously repeating local-space rotation.
///
/// Source lowering turns
/// source transform controllers into this consumer-shaped row so the runtime
/// never needs to interpret NIF controller graphs.
#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabRotationCycle {
    pub radians_per_second: [f32; 3],
}

/// An authored keyframe controller that drives an entity's local transform.
///
/// Channels without source keys keep the entity's authored value. All values
/// are already in Bevy axes.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct PrefabTransformAnimation {
    pub clock: PrefabControllerClock,
    pub translation_m: Option<[PrefabScalarCurve; 3]>,
    pub rotation: Option<PrefabRotationCurve>,
    pub uniform_scale: Option<PrefabScalarCurve>,
}

/// Source controller timing: `scaled = elapsed * frequency + phase`, then
/// wrapped into the start/stop interval by the cycle type in `flags`.
#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabControllerClock {
    pub flags: u16, // Bit 0 app-init clock, bits 1-2 loop/reverse/clamp, bit 3 active
    pub frequency: f32,
    pub phase: f32,
    pub start_time_s: f32,
    pub stop_time_s: f32,
}

/// Piecewise cubic curve. Each segment runs from its start time to the next
/// segment's start time; the final segment holds its value.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct PrefabScalarCurve {
    pub segments: Vec<PrefabScalarCurveSegment>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabScalarCurveSegment {
    pub start_time_s: f32,
    pub coefficients: [f32; 4], // Polynomial in the segment's normalized time, constant term first
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub enum PrefabRotationCurve {
    /// Angles in radians about Bevy X, Z and Y, composed in that order.
    EulerAnglesXzy([PrefabScalarCurve; 3]),
    /// Spherically interpolated keys, or held keys when `stepped`.
    Quaternions {
        keys: Vec<PrefabRotationKey>,
        stepped: bool,
    },
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabRotationKey {
    pub time_s: f32,
    pub rotation_xyzw: [f32; 4],
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct ScenePrefabRenderableVisibilityFlags(pub u8);

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct PrefabRenderable {
    pub model: AssetId,
    /// Stable glTF scene name within the referenced GLB. An empty name selects
    /// the document's default scene for assets that contain one render root.
    pub scene_name: String,
    pub material_overrides: Vec<PrefabRenderableMaterialOverride>,
    pub visibility: ScenePrefabRenderableVisibilityFlags,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefabRenderableMaterialOverride {
    pub slot: u16,
    pub material: AssetId,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct ScenePrefabColliderLayerFlags(pub u16);

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabCollider {
    pub source: PrefabColliderSource,
    pub layer: ScenePrefabColliderLayerFlags,
    pub mask: ScenePrefabColliderLayerFlags,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub enum PrefabColliderSource {
    Model { model: AssetId, index: u16 },
    Box { half_extent_m: [f32; 3] },
    Capsule { radius_m: f32, half_height_m: f32 },
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabLod {
    pub group_entity: u32,
    pub ordinal: u16,
    /// Authored local-space center used for camera-space depth selection.
    pub center_m: [f32; 3],
    pub near_m: f32,
    pub far_m: f32,
    pub active_without_range: bool,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefabBillboard {
    /// The source-authored billboard mode after format-default resolution.
    pub mode: u32,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabLight {
    pub kind: PrefabLightKind,
    pub color_srgb: [f32; 3],
    pub intensity: f32,
    pub range_m: f32,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrefabLightKind {
    Ambient,
    Directional,
    Point,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefabEffect {
    pub effect: AssetId,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct PrefabRailCamera {
    pub seconds_per_segment: f32,
    pub render_size: [u32; 2],
    pub keys: Vec<PrefabRailCameraKey>,
    pub environment_keys: Vec<PrefabEnvironmentKey>,
    pub directional_light_keys: Vec<PrefabDirectionalLightKey>,
    pub hotkeys: Vec<PrefabRailCameraHotkey>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabRailCameraKey {
    pub translation_m: [f32; 3],
    /// Source Euler angles retained in their authored XYZ convention. The
    /// Bevy consumer performs the one source-coordinate conversion.
    pub rotation_xyz: [f32; 3],
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabEnvironmentKey {
    pub time: f32,
    pub ambient_srgb: [f32; 3],
    pub shadow_strength: f32,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PrefabDirectionalLightKey {
    pub light: AssetId,
    pub time: f32,
    pub diffuse_srgb: [f32; 3],
    pub specular_srgb: [f32; 3],
    pub direction: [f32; 3],
    pub intensity: f32,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefabRailCameraHotkey {
    pub key_code: u16,
    pub triggered_on_press: bool,
    pub active: bool,
    pub command: PrefabRailCameraCommand,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PrefabRailCameraCommand {
    Left,
    Right,
    Up,
    Down,
    In,
    Out,
    RollLeft,
    RollRight,
    Record,
    Play,
    Save,
    Load,
    Clear,
    Fast,
}
