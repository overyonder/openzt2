use bevy::{prelude::*, render::Extract, ui::ComputedStackIndex, ui_render::ExtractedUiNodes};

use super::authored_ui_visual_types::{UiVisualBackgroundSlotOwner, UiVisualLayer};
use bevy::ui_render::ui_texture_slice_pipeline::ExtractedUiTextureSlices;

/// Authored aspect layers are their owner's background, not foreground children.
/// Bevy's stack visits parents before children regardless of negative ZIndex.
/// Retain Bevy's per-item image/border offsets within the owner's stack slot.
pub fn place_authored_aspect_draws_in_owner_stack_slot(
    layers: Extract<Query<&ChildOf, With<UiVisualLayer>>>,
    nodes: Extract<Query<(&ComputedStackIndex, Option<&UiVisualBackgroundSlotOwner>)>>,
    mut extracted: ResMut<ExtractedUiNodes>,
    mut sliced: ResMut<ExtractedUiTextureSlices>,
) {
    for draw in &mut extracted.uinodes {
        let Some((owner_stack, draw_stack)) =
            authored_background_stack_indices(draw.main_entity.id(), &layers, &nodes)
        else {
            continue;
        };
        draw.z_order += owner_stack as f32 - draw_stack as f32;
    }
    // Bevy queues scalable skins through its separate texture-slice pipeline.
    // That pipeline adds the image offset after reading this stack index.
    for draw in &mut sliced.slices {
        if let Some((owner_stack, _)) =
            authored_background_stack_indices(draw.main_entity.id(), &layers, &nodes)
        {
            draw.stack_index = owner_stack;
        }
    }
}

fn authored_background_stack_indices(
    entity: Entity,
    layers: &Query<&ChildOf, With<UiVisualLayer>>,
    nodes: &Query<(&ComputedStackIndex, Option<&UiVisualBackgroundSlotOwner>)>,
) -> Option<(u32, u32)> {
    let (draw_stack, explicit_owner) = nodes.get(entity).ok()?;
    let owner = if let Some(owner) = explicit_owner {
        owner.0
    } else {
        let parent = layers.get(entity).ok()?.parent();
        nodes.get(parent).ok()?.1.map_or(parent, |owner| owner.0)
    };
    let (owner_stack, _) = nodes.get(owner).ok()?;
    Some((owner_stack.0, draw_stack.0))
}
