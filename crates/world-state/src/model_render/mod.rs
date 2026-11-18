use bevy::prelude::*;

pub mod authored_effect_technique_pass_submission_order;
pub mod authored_model_material_pass_projection;
pub mod environment_render_layer;

#[derive(Component)]
pub struct ModelExpanded;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelRenderPresentationSet {
    LevelOfDetail,
    /// The source effect materials and their per-model texture replacements are ready.
    SourceMaterials,
    /// Queues changes to scene caustic draws and suppressed render layers.
    VisibilityInputCommands,
    /// Applies those commands before visibility reads their results.
    ApplyVisibilityInputCommands,
}
