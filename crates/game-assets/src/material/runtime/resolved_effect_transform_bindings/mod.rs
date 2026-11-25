//! Render-only operations resolved from effect reflection at asset load time.

use bevy::prelude::*;

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
}

/// A camera's fog registers, for renderers that set them on the CPU.
#[derive(Clone, Copy, Debug)]
pub struct EffectRenderViewTransforms {
    view_to_world: Mat4,
    camera_position: Vec3,
}

impl EffectRenderViewTransforms {
    pub fn fog_shader_registers(
        &self,
        fog: &bevy::pbr::DistanceFog,
        projection: &Projection,
    ) -> Option<[Vec4; 2]> {
        let bevy::pbr::FogFalloff::Linear { start, end } = fog.falloff else {
            return None;
        };
        let direction = -self.view_to_world.z_axis.truncate().normalize();
        let far = match projection {
            Projection::Perspective(projection) => projection.far,
            Projection::Orthographic(projection) => projection.far,
            Projection::Custom(_) => end,
        };
        Some([
            Vec4::new(start, (end - start).recip(), end, far),
            direction.extend(self.camera_position.dot(direction)),
        ])
    }

    pub fn from_camera(transform: &GlobalTransform, _projection: &Projection) -> Self {
        Self {
            view_to_world: Mat4::from(transform.affine()),
            camera_position: transform.translation(),
        }
    }
}
