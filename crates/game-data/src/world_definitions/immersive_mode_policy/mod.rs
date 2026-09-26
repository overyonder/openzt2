//! Authored immersive-mode input, tool presentation, camera, and lifecycle policy.

use crate::AssetId;

mod immersive_mode_action_flag_operations;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum ImmersiveModeKind {
    GuestView,
    FirstPerson,
    Training,
    FossilSearch,
    FossilAssembly,
    Cloning,
    Photo,
    ShowEdit,
    SuperStaff,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct ImmersiveModeActionFlags(u32);

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "camera restoration, pausing, HUD visibility and subject locking are independent authored policies"
)]
pub struct ImmersiveModePolicy {
    pub id: AssetId,
    pub mode: ImmersiveModeKind,
    pub allowed_actions: ImmersiveModeActionFlags,
    pub interaction_cursor: AssetId,
    pub interaction_prefab: AssetId,
    pub camera: AssetId,
    pub restore_camera_on_exit: bool,
    pub pause_simulation: bool,
    pub hide_hud: bool,
    pub lock_subject: bool,
    /// Number of presented frames for which Photo mode keeps the most recent
    /// capture visible in its authored preview surface.
    pub last_captured_photo_preview_frame_count: u16,
    /// Frustum-scale delta per wheel event authored by ZTPhotoMode.
    #[serde(default)]
    pub photo_zoom_step: f32,
}
