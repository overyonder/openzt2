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
pub struct EffectRenderViewTransforms {
    pub(super) world_to_view: Mat4,
    pub(super) view_to_world: Mat4,
    pub(super) view_to_projection: Mat4,
    pub(super) world_to_projection: Mat4,
    pub(super) camera_position: Vec3,
    pub(super) camera_across: Vec3,
    pub(super) camera_up: Vec3,
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
    pub fn view_to_world_components(&self) -> [f32; 16] {
        self.view_to_world.to_cols_array()
    }

    pub fn from_camera(transform: &GlobalTransform, projection: &Projection) -> Self {
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

/// Lazy calculations shared by fixed-function and programmable inputs for one draw.
pub(super) struct EffectDrawTransforms<'a> {
    model_to_world: Mat4,
    view: &'a EffectRenderViewTransforms,
    world_to_model: std::cell::OnceCell<Mat4>,
    model_to_view: std::cell::OnceCell<Mat4>,
    model_to_projection: std::cell::OnceCell<Mat4>,
}

impl<'a> EffectDrawTransforms<'a> {
    pub(super) fn new(model_to_world: Mat4, view: &'a EffectRenderViewTransforms) -> Self {
        Self {
            model_to_world,
            view,
            world_to_model: Default::default(),
            model_to_view: Default::default(),
            model_to_projection: Default::default(),
        }
    }

    pub(super) fn vector(&self, semantic: ResolvedEffectTransformSemantic) -> Option<Vec4> {
        use ResolvedEffectTransformSemantic as Semantic;
        match semantic {
            Semantic::CameraPosition => Some(self.view.camera_position.extend(1.0)),
            Semantic::CameraAcross => Some(self.view.camera_across.extend(0.0)),
            Semantic::CameraUp => Some(self.view.camera_up.extend(0.0)),
            _ => None,
        }
    }

    pub(super) fn matrix(&self, semantic: ResolvedEffectTransformSemantic) -> Option<Mat4> {
        use ResolvedEffectTransformSemantic as Semantic;
        Some(match semantic {
            Semantic::Identity => Mat4::IDENTITY,
            Semantic::ModelToWorld => self.model_to_world,
            Semantic::WorldToModel => *self
                .world_to_model
                .get_or_init(|| self.model_to_world.inverse()),
            Semantic::VectorToWorld => self
                .world_to_model
                .get_or_init(|| self.model_to_world.inverse())
                .transpose(),
            Semantic::VectorWorldToView => self.view.view_to_world.transpose(),
            Semantic::ModelToView => *self
                .model_to_view
                .get_or_init(|| self.view.world_to_view * self.model_to_world),
            Semantic::ModelToProjection => *self
                .model_to_projection
                .get_or_init(|| self.view.world_to_projection * self.model_to_world),
            Semantic::WorldToView => self.view.world_to_view,
            Semantic::ViewToWorld => self.view.view_to_world,
            Semantic::ViewToProjection => self.view.view_to_projection,
            Semantic::WorldToProjection => self.view.world_to_projection,
            _ => return None,
        })
    }
}
