use bevy::prelude::*;

/// Base textures bound in place of the authored ones for every renderable
/// beneath this entity whose lowercase source material name matches.
#[derive(Component, Debug, Clone)]
pub struct PrefabTextureReplacements(pub Box<[(Box<str>, Handle<Image>)]>);
