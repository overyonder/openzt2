use serde::{Deserialize, Serialize};

use crate::AssetId;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnimationBinderRig {
    pub definition: AssetId,
    pub binder: AssetId,
    pub quadruped_ground_fit: Option<GroundFitRig>,
    pub biped_ground_fit: Option<GroundFitRig>,
    pub spine_bend: Option<SpineBendRig>,
    pub decal_cycles: Vec<DecalTextureCycle>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GroundFitRig {
    pub support: GroundFitSupport,
    pub height_offset: f32,
    pub height_offset_scale: f32,
    pub height_offset_disabled_seconds: f32,
    pub tilt_off: bool,
    pub allowed_roll_radians: f32,
    /// Height, pitch and roll.
    pub error_limits: [f32; 3],
    pub gains: [f32; 3],
    pub buoyancy: [Option<AssetId>; 2],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum GroundFitSupport {
    Biped {
        ankles: [Option<AssetId>; 2],
        ankle_axes: [[f32; 3]; 2],
        fit_radius: f32,
    },
    Quadruped {
        bicycle_points: [Option<AssetId>; 2],
        foot_spacing: f32,
        /// Left front, right front, left back, right back.
        legs: Option<QuadrupedLegFit>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct QuadrupedLegFit {
    pub initially_enabled: bool,
    pub maximum_raise: f32,
    pub legs: [GroundFitLeg; 4],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GroundFitLeg {
    pub hip: Option<AssetId>,
    pub knee: Option<AssetId>,
    pub ankle: Option<AssetId>,
    pub hip_axis: [f32; 3],
    pub knee_axis: [f32; 3],
    pub ankle_axis: [f32; 3],
    pub fit_mode: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpineBendRig {
    pub base: AssetId,
    pub spine: Vec<AssetId>,
    pub tail: Vec<AssetId>,
    pub maximum_bend_radians: f32,
    pub bend_radius: Option<f32>,
    pub blend_seconds: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DecalTextureCycle {
    pub material: Box<str>,
    pub layer: Option<usize>,
    pub texture_coordinates: Option<u32>,
    pub enabled: bool,
    pub frozen: bool,
    pub stop_at: Option<usize>,
    pub steps: Vec<DecalTextureStep>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DecalTextureStep {
    pub image: Box<str>,
    pub hold_seconds: f32,
    pub variance_seconds: f32,
}
