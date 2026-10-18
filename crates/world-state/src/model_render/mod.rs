use bevy::prelude::*;

pub mod authored_effect_technique_pass_submission_order;
pub mod authored_model_material_pass_projection;

#[derive(Component)]
pub struct ModelExpanded;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelRenderPresentationSet {
    LevelOfDetail,
    MaterialProjection,
    PassTransforms,
}
