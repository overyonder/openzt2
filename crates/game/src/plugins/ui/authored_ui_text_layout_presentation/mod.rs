use bevy::{
    prelude::*,
    text::{FontCx, TextLayoutInfo},
};

pub(crate) const AUTHORED_COMIC_SANS_COMPATIBLE_FONT_FAMILY: &str = "Comic Neue";

#[derive(Component)]
pub(super) struct UiSingleLineTextAlignment;

/// Authored font height retained for localized rich-text descendants. Glyphs
/// follow the canvas's vertical scale without acquiring its horizontal stretch.
#[derive(Component, Clone, Copy)]
pub(crate) struct UiPhysicalFontSize(f32);

impl UiPhysicalFontSize {
    pub(crate) fn from_physical_pixels(physical_pixels: f32) -> Self {
        Self(physical_pixels)
    }

    pub(crate) fn physical_pixels(&self) -> f32 {
        self.0
    }
}

/// Register the bundled Comic Sans-compatible face and bind the remaining
/// authored Windows family role to an installed platform face once. Arial's
/// metrically compatible Liberation Sans replacement preserves authored text
/// advances on platforms where Arial itself is absent.
pub(super) fn bind_authored_windows_font_roles_to_available_platform_font_families(
    mut font_assets: ResMut<Assets<Font>>,
    mut fonts: ResMut<FontCx>,
) {
    // Bevy prunes inactive source entries after two frames. Keep Fontique's
    // weak backing cache so reloading a face still held by a text layout reuses
    // its Blob identity and existing glyph atlases.
    fonts.source_cache.make_shared();
    font_assets.add(Font::from_bytes(
        include_bytes!("../../../../assets/fonts/ComicNeue-Regular.ttf").to_vec(),
    ));

    let sans = ["Arial", "Liberation Sans"]
        .into_iter()
        .find(|family| fonts.collection.family_id(family).is_some());
    if let Some(family) = sans {
        if let Err(error) = fonts.set_sans_serif_family(family) {
            warn!("failed to bind authored Arial role to {family}: {error}");
        }
    } else {
        warn!("neither Arial nor Liberation Sans is available; using the platform sans-serif face");
    }
}

/// Aligns Blue Fang's ordinary single-line text inside its authored region.
///
/// Bevy deliberately gives `NoWrap` text unbounded layout bounds, so its
/// horizontal `Justify` cannot observe the UI node's authored width. Preserve
/// the no-wrap contract and use Bevy's measured glyph run to supply the
/// missing horizontal offset instead of changing line breaking or baking
/// font-specific positions.
pub(super) fn align_authored_single_line_text_from_measured_glyph_run(
    mut texts: Query<
        (
            &ComputedNode,
            &TextLayout,
            &TextLayoutInfo,
            &mut UiTransform,
        ),
        With<UiSingleLineTextAlignment>,
    >,
) {
    for (node, text_layout, layout, mut transform) in &mut texts {
        let remaining = (node.content_box().size() - layout.size).max(Vec2::ZERO);
        let horizontal = match text_layout.justify {
            Justify::Center => remaining.x * 0.5,
            Justify::End | Justify::Right => remaining.x,
            Justify::Start | Justify::Left | Justify::Justified => 0.0,
        };
        let inverse_scale = node.inverse_scale_factor();
        // BF text begins at the authored region's top edge. Its `align`
        // attribute controls only the inline axis; vertically centering the
        // measured run moves short labels behind lower visual borders (most
        // visibly the photo-album page numbers).
        let next = Val2::px(horizontal * inverse_scale, 0.0);
        if transform.translation != next {
            transform.translation = next;
        }
    }
}
