use bevy::prelude::*;

/// Progress displayed by a fossil education center. The original
/// `f_BoneLevel` attribute is projected directly onto the center entity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct FossilEducationProgress {
    pub bone_level: f32,
}

/// Authored `BFAIEntityDataShared b_Super="true"` fact on the extinct-animal
/// entity itself. The original photo API exposes this exact boolean through
/// `getIsSuper`; it is not inferred from species names or unlock state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuthoredSuperExtinctAnimal;
