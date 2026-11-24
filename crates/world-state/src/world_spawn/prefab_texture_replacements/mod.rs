use bevy::prelude::*;

/// Base textures bound in place of the authored ones for every renderable
/// beneath this entity whose lowercase source material name matches.
#[derive(Component, Debug, Clone)]
pub struct PrefabTextureReplacements(pub Box<[(Box<str>, Handle<Image>)]>);

/// Decal textures drawn over the authored base textures of every renderable
/// beneath this entity whose lowercase source material name matches.
#[derive(Component, Debug, Clone)]
pub struct PrefabDecalTextures(pub Box<[(Box<str>, Handle<Image>)]>);
