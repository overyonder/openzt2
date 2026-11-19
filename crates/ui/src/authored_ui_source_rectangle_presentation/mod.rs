use bevy::{asset::asset_changed::AssetChanged, prelude::*};

use super::authored_ui_visual_types::UiSourceRect;

/// Resolves authored sampling bounds when an image or its source rectangle
/// changes. Native sprite binding clips oversized source regions to the loaded
/// texture before UV normalization; destination widget dimensions stay intact.
pub(super) fn project_changed_authored_image_source_rectangles(
    images: Res<Assets<Image>>,
    mut nodes: Query<
        (&UiSourceRect, &mut ImageNode),
        Or<(
            Changed<UiSourceRect>,
            Changed<ImageNode>,
            AssetChanged<ImageNode>,
        )>,
    >,
) {
    for (source, node) in &mut nodes {
        if let Some(image) = images.get(&node.image) {
            let rectangle = resolve_authored_source_rectangle(source.0, image.size());
            node.map_unchanged(|node| &mut node.rect)
                .set_if_neq(Some(rectangle));
        }
    }
}

fn resolve_authored_source_rectangle(source: [i32; 4], size: UVec2) -> Rect {
    let extent = size.as_vec2();
    if source[2] <= 0 && source[3] <= 0 {
        return Rect::from_corners(Vec2::ZERO, extent);
    }
    // Clamp the origin to the last texel and then limit
    // width/height to the remaining extent. It does not resize the texture.
    let origin =
        Vec2::new(source[0] as f32, source[1] as f32).min((extent - Vec2::ONE).max(Vec2::ZERO));
    let dimensions = Vec2::new(source[2] as f32, source[3] as f32).min(extent - origin);
    Rect::from_corners(origin, origin + dimensions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_zoopedia_source_uses_the_loaded_image_extent() {
        assert_eq!(
            resolve_authored_source_rectangle([0, 0, 256, 256], UVec2::splat(128)),
            Rect::new(0.0, 0.0, 128.0, 128.0)
        );
    }

    #[test]
    fn atlas_bounds_and_source_size_defaults_retain_native_mapping() {
        assert_eq!(
            resolve_authored_source_rectangle([64, 32, 16, 24], UVec2::splat(128)),
            Rect::new(64.0, 32.0, 80.0, 56.0)
        );
        assert_eq!(
            resolve_authored_source_rectangle([160, 120, 64, 64], UVec2::splat(128)),
            Rect::new(127.0, 120.0, 128.0, 128.0)
        );
        assert_eq!(
            resolve_authored_source_rectangle([0, 0, -1, -1], UVec2::new(128, 64)),
            Rect::new(0.0, 0.0, 128.0, 64.0)
        );
    }
}
