use bevy::{camera::visibility::RenderLayers, prelude::*};

pub(super) fn render_layers_for_prefab_entity_and_ancestor_chain(
    mut entity: Entity,
    hierarchy: &Query<(Option<&ChildOf>, Option<&RenderLayers>)>,
) -> RenderLayers {
    for _ in 0..64 {
        let Ok((parent, layers)) = hierarchy.get(entity) else {
            break;
        };
        if let Some(layers) = layers {
            return layers.clone();
        }
        let Some(parent) = parent else {
            break;
        };
        entity = parent.parent();
    }
    RenderLayers::default()
}
