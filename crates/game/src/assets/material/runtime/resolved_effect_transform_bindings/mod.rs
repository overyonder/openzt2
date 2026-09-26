//! Render-only operations resolved from effect reflection at asset load time.

use bevy::prelude::*;

pub(super) const MODEL_INPUT: u8 = 1;
pub(super) const VIEW_INPUT: u8 = 2;
pub(super) const WIND_INPUT: u8 = 4;
pub(super) const LIGHT_INPUT: u8 = 8;
pub(super) const FOG_INPUT: u8 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResolvedEffectTransformSemantic {
    Identity,
    ModelToWorld,
    WorldToModel,
    VectorToWorld,
    VectorWorldToView,
    ModelToView,
    ModelToProjection,
    WorldToView,
    ViewToWorld,
    ViewToProjection,
    WorldToProjection,
    CameraPosition,
    CameraAcross,
    CameraUp,
    Wind,
    Light,
    Fog,
    Other,
}

impl ResolvedEffectTransformSemantic {
    pub(super) fn from_authored_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "identitymatrix" => Self::Identity,
            "localtoworld" | "modeltoworld" => Self::ModelToWorld,
            "worldtomodel" => Self::WorldToModel,
            "vectoworld" => Self::VectorToWorld,
            "vecworldtoview" => Self::VectorWorldToView,
            "modeltoview" => Self::ModelToView,
            "modeltoproj" => Self::ModelToProjection,
            "worldtocamera" | "worldtoview" => Self::WorldToView,
            "viewtoworld" => Self::ViewToWorld,
            "cameratondc" | "viewtoproj" => Self::ViewToProjection,
            "worldtondc" | "worldtoproj" => Self::WorldToProjection,
            "camerapos" => Self::CameraPosition,
            "cameraacross" => Self::CameraAcross,
            "cameraup" => Self::CameraUp,
            "windblock" => Self::Wind,
            "lightrig" => Self::Light,
            "fogparams" => Self::Fog,
            _ => Self::Other,
        }
    }

    pub(super) const fn input_dependencies(self) -> u8 {
        match self {
            Self::Identity | Self::Other => 0,
            Self::ModelToWorld | Self::WorldToModel | Self::VectorToWorld => MODEL_INPUT,
            Self::ModelToView | Self::ModelToProjection => MODEL_INPUT | VIEW_INPUT,
            Self::Wind => WIND_INPUT,
            Self::Light => LIGHT_INPUT,
            Self::Fog => FOG_INPUT | VIEW_INPUT,
            _ => VIEW_INPUT,
        }
    }
}

/// Shared camera calculations for all effect draws belonging to one view.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EffectRenderViewTransforms {
    pub(super) world_to_view: Mat4,
    pub(super) view_to_world: Mat4,
    pub(super) view_to_projection: Mat4,
    pub(super) world_to_projection: Mat4,
    pub(super) camera_position: Vec3,
    pub(super) camera_across: Vec3,
    pub(super) camera_up: Vec3,
}

impl EffectRenderViewTransforms {
    pub(crate) fn from_camera(transform: &GlobalTransform, projection: &Projection) -> Self {
        let world_to_view = Mat4::from(transform.affine().inverse());
        let view_to_projection = projection.get_clip_from_view();
        Self {
            world_to_view,
            view_to_world: world_to_view.inverse(),
            view_to_projection,
            world_to_projection: view_to_projection * world_to_view,
            camera_position: transform.translation(),
            camera_across: transform.right().as_vec3(),
            camera_up: transform.up().as_vec3(),
        }
    }
}
