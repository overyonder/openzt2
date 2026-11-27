use super::animation_text_key::AuthoredAnimationTextKey;

/// Authored metadata which accompanies one standard glTF animation clip.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationClipMetadata {
    pub animation_clip_asset_path: String,
    pub animation_clip_asset_key: String,
    pub duration_milliseconds: u32,
    pub playback_is_looped: bool,
    pub additive_blending_is_enabled: bool,
    pub root_motion_transform: Option<AuthoredAnimationRootMotionTransform>,
    #[serde(default)]
    pub root_motion_joint: Option<AuthoredAnimationRootMotionJoint>,
    pub animation_markers: Vec<AuthoredAnimationMarker>,
    pub authored_animation_text_keys: Vec<AuthoredAnimationTextKey>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationRootMotionJoint {
    pub joint_asset_key: String,
    pub initial_translation: [f32; 3],
}

impl AuthoredAnimationRootMotionJoint {
    /// Skeleton joints are identified by their lowercased names, so a clip
    /// naming `Bip01` drives the model's `bip01` joint.
    #[must_use]
    pub fn joint_id(&self) -> crate::AssetId {
        crate::AssetId::from_key(&self.joint_asset_key.to_ascii_lowercase())
    }
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationRootMotionTransform {
    pub translation_from_first_to_last_frame: [f32; 3],
    pub rotation_from_first_to_last_frame: [f32; 4],
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AuthoredAnimationMarker {
    pub event_time_milliseconds: u32,
    pub marker_name: String,
}
