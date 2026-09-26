use openzt2_game_data::audio::{AudioUsage, AudioVariant, VARIANT_CUE_REFERENCE};

use crate::assets::audio::audio_asset_types::{AudioView, ResolvedAudioCue};

pub(super) fn resolve_weighted_audio_cue_variant<'a>(
    audio_asset_view: AudioView<'a>,
    cue: openzt2_game_data::AssetId,
    selection: u64,
    reference_depth: u8,
) -> Option<(ResolvedAudioCue<'a>, &'a AudioVariant, AudioUsage)> {
    if reference_depth == 8 {
        return None;
    }
    let resolved_cue = audio_asset_view.cue(cue)?;
    let total_variant_weight = resolved_cue
        .cue
        .variants
        .iter()
        .map(|variant| variant.weight.max(0.0))
        .sum::<f32>();
    if total_variant_weight <= 0.0 {
        return None;
    }
    let selected_weight_offset = ((selection.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 40) as f32
        / (1_u64 << 24) as f32)
        * total_variant_weight;
    let mut cumulative_variant_weight = 0.0;
    let selected_variant = resolved_cue
        .cue
        .variants
        .iter()
        .find(|variant| {
            cumulative_variant_weight += variant.weight.max(0.0);
            selected_weight_offset <= cumulative_variant_weight
        })
        .or_else(|| resolved_cue.cue.variants.last())?;
    if selected_variant.flags & VARIANT_CUE_REFERENCE != 0 {
        resolve_weighted_audio_cue_variant(
            audio_asset_view,
            selected_variant.source_id,
            selection.rotate_left(13),
            reference_depth + 1,
        )
    } else {
        Some((resolved_cue, selected_variant, resolved_cue.cue.usage))
    }
}
