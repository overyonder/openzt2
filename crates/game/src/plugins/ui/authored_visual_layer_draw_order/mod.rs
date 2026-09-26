use bevy::{prelude::*, render::Extract, ui::ComputedStackIndex, ui_render::ExtractedUiNodes};

use super::authored_ui_visual_types::UiVisualLayer;
use bevy::ui_render::ui_texture_slice_pipeline::ExtractedUiTextureSlices;

/// Authored aspect layers are their owner's background, not foreground children.
/// Bevy's stack visits parents before children regardless of negative ZIndex.
/// Retain Bevy's per-item image/border offsets within the owner's stack slot.
pub(super) fn place_authored_aspect_draws_in_owner_stack_slot(
    layers: Extract<Query<(&ChildOf, &ComputedStackIndex), With<UiVisualLayer>>>,
    owners: Extract<Query<&ComputedStackIndex>>,
    mut extracted: ResMut<ExtractedUiNodes>,
    mut sliced: ResMut<ExtractedUiTextureSlices>,
) {
    for draw in &mut extracted.uinodes {
        let Ok((parent, layer_stack)) = layers.get(draw.main_entity.id()) else {
            continue;
        };
        let Ok(owner_stack) = owners.get(parent.parent()) else {
            continue;
        };
        draw.z_order += owner_stack.0 as f32 - layer_stack.0 as f32;
    }
    // Bevy queues scalable skins through its separate texture-slice pipeline.
    // That pipeline adds the image offset after reading this stack index.
    for draw in &mut sliced.slices {
        let Ok((parent, _)) = layers.get(draw.main_entity.id()) else {
            continue;
        };
        if let Ok(owner_stack) = owners.get(parent.parent()) {
            draw.stack_index = owner_stack.0;
        }
    }
}
