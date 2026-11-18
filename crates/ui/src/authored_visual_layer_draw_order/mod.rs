use bevy::{
    prelude::*,
    render::{sync_world::MainEntityHashSet, Extract},
    ui::ComputedStackIndex,
    ui_render::ExtractedUiNodes,
};

use super::authored_ui_visual_types::{UiVisualBackgroundSlotOwner, UiVisualLayer};
use bevy::ui_render::ui_texture_slice_pipeline::ExtractedUiTextureSlices;

/// Authored aspect layers are their owner's background, not foreground children.
/// Bevy's stack visits parents before children regardless of negative ZIndex.
/// Retain Bevy's per-item image/border offsets within the owner's stack slot.
pub fn place_authored_aspect_draws_in_owner_stack_slot(
    layers: Extract<Query<&ChildOf, With<UiVisualLayer>>>,
    nodes: Extract<
        Query<(
            Ref<ComputedStackIndex>,
            Option<&UiVisualBackgroundSlotOwner>,
        )>,
    >,
    extracted: ResMut<ExtractedUiNodes>,
    mut sliced: ResMut<ExtractedUiTextureSlices>,
    mut reordered: Local<MainEntityHashSet>,
) {
    let ExtractedUiNodes { uinodes, changed } = extracted.into_inner();
    // A sibling reorder can move the owner while a background retains its own index.
    let entities = if changed.iter().any(|entity| {
        nodes
            .get(entity.entity())
            .is_ok_and(|(stack, _)| stack.is_changed())
    }) {
        reordered.clear();
        reordered.extend(uinodes.keys().chain(sliced.slices.keys()).copied());
        &*reordered
    } else {
        &*changed
    };
    for entity in entities {
        let Some(owner_stack) = authored_background_stack_index(entity.entity(), &layers, &nodes)
        else {
            continue;
        };
        if let Some((_, draws)) = uinodes.get_mut(entity) {
            for draw in draws.values_mut() {
                // Bevy offsets each draw by less than half a stack slot.
                let offset = draw.z_order - draw.z_order.round();
                draw.z_order = owner_stack as f32 + offset;
            }
        }
        if let Some((_, draws)) = sliced.slices.get_mut(entity) {
            for draw in draws.values_mut() {
                draw.stack_index = owner_stack;
            }
        }
    }
}

fn authored_background_stack_index(
    entity: Entity,
    layers: &Query<&ChildOf, With<UiVisualLayer>>,
    nodes: &Query<(
        Ref<ComputedStackIndex>,
        Option<&UiVisualBackgroundSlotOwner>,
    )>,
) -> Option<u32> {
    let (_, explicit_owner) = nodes.get(entity).ok()?;
    let owner = if let Some(owner) = explicit_owner {
        owner.0
    } else {
        let parent = layers.get(entity).ok()?.parent();
        nodes.get(parent).ok()?.1.map_or(parent, |owner| owner.0)
    };
    let (owner_stack, _) = nodes.get(owner).ok()?;
    Some(owner_stack.0)
}
