use bevy::prelude::*;

/// Base textures bound in place of the authored ones for every renderable
/// beneath this entity whose lowercase source material name matches.
#[derive(Component, Debug, Clone)]
pub(crate) struct PrefabTextureReplacements(pub(crate) Box<[(Box<str>, Handle<Image>)]>);
