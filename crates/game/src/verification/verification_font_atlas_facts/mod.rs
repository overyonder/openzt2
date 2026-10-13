use bevy::{
    prelude::*,
    text::{ComputedTextBlock, FontAtlasSet, FontHinting, FontSmoothing},
    window::PrimaryWindow,
};

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

pub(super) fn collect_font_atlas_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    atlases: Res<FontAtlasSet>,
    images: Res<Assets<Image>>,
    fonts: Res<Assets<Font>>,
    texts: Query<
        (
            &ComputedTextBlock,
            &FontHinting,
            &InheritedVisibility,
            &ComputedNode,
        ),
        With<Text>,
    >,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let count = |value: usize| f64::from(u32::try_from(value).unwrap_or(u32::MAX));
    let mut texture_count = 0;
    let mut glyph_count = 0;
    let mut missing_cpu_images = 0;
    let mut capacity_bytes = 0_u64;
    for atlas in atlases.values().flatten() {
        texture_count += 1;
        glyph_count += atlas.glyph_to_atlas_index.len();
        if let Some(data) = images
            .get(&atlas.texture)
            .and_then(|image| image.data.as_ref())
        {
            capacity_bytes += u64::try_from(data.capacity()).unwrap_or(u64::MAX);
        } else {
            missing_cpu_images += 1;
        }
    }
    for (name, value) in [
        ("font_atlas_keys", count(atlases.len())),
        ("font_atlas_textures", count(texture_count)),
        ("font_atlas_cached_glyphs", count(glyph_count)),
        ("font_atlas_missing_cpu_images", count(missing_cpu_images)),
        ("font_asset_count", count(fonts.len())),
        ("image_asset_count", count(images.len())),
    ] {
        report.current_facts.insert(name.to_owned(), value);
    }
    // These are CPU pixel payload and reserved Vec storage, respectively.
    // Neither includes allocator overhead, GPU copies, font shaping caches or RSS.
    #[allow(clippy::cast_precision_loss)]
    let data_bytes = atlases.total_bytes(&images) as f64;
    #[allow(clippy::cast_precision_loss)]
    let capacity_bytes = capacity_bytes as f64;
    report
        .current_facts
        .insert("font_atlas_cpu_data_bytes".to_owned(), data_bytes);
    report
        .current_facts
        .insert("font_atlas_cpu_capacity_bytes".to_owned(), capacity_bytes);

    // Borrow the shaped runs, including fallback face identity and actual raster
    // size. Sort the glyph demand so entity reconstruction/order cannot affect it.
    // Use extraction's visibility/empty-node eligibility; include whole shaped
    // runs even when glyphs are clipped. This measures demand, not final pixels.
    let mut glyph_inputs = Vec::new();
    for (block, hinting, visibility, node) in &texts {
        if !visibility.get() || node.is_empty() {
            continue;
        }
        let layout = block.buffer();
        for line in layout.lines() {
            for run in line.runs() {
                let mut run_hash = blake3::Hasher::new();
                run_hash.update(&run.font().data.id().to_le_bytes());
                run_hash.update(&run.font().index.to_le_bytes());
                run_hash.update(&run.font_size().to_bits().to_le_bytes());
                run_hash.update(&[u8::from(hinting.is_enabled())]);
                run_hash.update(&run.normalized_coords().len().to_le_bytes());
                for coordinate in run.normalized_coords() {
                    run_hash.update(&coordinate.to_le_bytes());
                }
                for cluster in run.clusters() {
                    for glyph in cluster.glyphs() {
                        let mut glyph_hash = run_hash.clone();
                        glyph_hash.update(&glyph.id.to_le_bytes());
                        glyph_hash.update(&[u8::from(matches!(
                            layout.styles()[glyph.style_index()].brush.font_smoothing,
                            FontSmoothing::AntiAliased
                        ))]);
                        glyph_inputs.push(*glyph_hash.finalize().as_bytes());
                    }
                }
            }
        }
    }
    glyph_inputs.sort_unstable();
    glyph_inputs.dedup();
    report
        .current_facts
        .insert("font_ui_glyph_inputs".to_owned(), count(glyph_inputs.len()));
    let mut hash = blake3::Hasher::new();
    if let Ok(window) = windows.single() {
        hash.update(&window.physical_width().to_le_bytes());
        hash.update(&window.physical_height().to_le_bytes());
        hash.update(&window.scale_factor().to_bits().to_le_bytes());
    }
    hash.update(&ui_scale.0.to_bits().to_le_bytes());
    for glyph in glyph_inputs {
        hash.update(&glyph);
    }
    for (index, bytes) in hash.finalize().as_bytes()[..16].chunks_exact(4).enumerate() {
        let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        report.current_facts.insert(
            format!("font_ui_glyph_input_identity_{index}"),
            f64::from(value),
        );
    }
}
