use bevy::{camera::visibility::RenderLayers, prelude::*};

use super::placed_object_relocation_model_presentation::RelocationModelRenderLayers;

pub(super) fn render_layers_for_prefab_entity_and_ancestor_chain(
    mut entity: Entity,
    hierarchy: &Query<(
        Option<&ChildOf>,
        Option<&RenderLayers>,
        Option<&RelocationModelRenderLayers>,
    )>,
) -> RenderLayers {
    for _ in 0..64 {
        let Ok((parent, layers, relocation)) = hierarchy.get(entity) else {
            break;
        };
        // Camera-dependent presentation keeps using the ordinary policy while
        // relocation suppresses draws. In particular, it must not retire LODs.
        if let Some(layers) = relocation.map_or(layers, |saved| saved.0.as_ref()) {
            return layers.clone();
        }
        let Some(parent) = parent else {
            break;
        };
        entity = parent.parent();
    }
    RenderLayers::default()
}
