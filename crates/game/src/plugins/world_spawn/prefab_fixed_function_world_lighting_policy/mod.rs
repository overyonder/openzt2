use bevy::prelude::*;

/// Per-presentation override for the global D3D fixed-function lighting state.
///
/// Blue Fang sky layers use this policy to distinguish vertex-coloured domes
/// from cloud layers which deliberately consume the world's lights.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrefabFixedFunctionWorldLightingPolicy {
    pub(crate) enabled: bool,
    pub(crate) preserve_authored_material_lighting: bool,
}
