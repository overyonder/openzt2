use bevy::prelude::*;

/// A model was replaced or restored while the game runs, such as one sent
/// from Blender. Objects using it rebuild their render parts once it has
/// reloaded. Ordinary reloads, such as a model coming back into view, do not
/// send this.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct PrefabModelReplaced {
    /// The model's asset path, without a label.
    pub model_path: String,
}
