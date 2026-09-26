use bevy::prelude::*;

use super::super::placement_preview_types::ObjectPlacementPreviewMotion;

#[test]
fn placement_preview_uses_the_configured_linear_interpolation_rate() {
    let mut motion = ObjectPlacementPreviewMotion {
        current: Vec3::ZERO,
        start: Vec3::ZERO,
        target: Vec3::X,
        progress: 0.0,
        progress_per_second: ObjectPlacementPreviewMotion::progress_per_second(1.0),
    };
    motion.advance(1.0 / 34.0);
    assert_eq!(motion.progress, 0.5);
    assert_eq!(motion.current, Vec3::new(0.5, 0.0, 0.0));
    motion.advance(1.0);
    assert_eq!(motion.progress, 1.0);
    assert_eq!(motion.current, Vec3::X);
}

#[test]
fn placement_preview_uses_authored_positive_weight_and_original_fallback() {
    assert_eq!(ObjectPlacementPreviewMotion::progress_per_second(2.0), 8.5);
    assert_eq!(ObjectPlacementPreviewMotion::progress_per_second(0.0), 17.0);
    assert_eq!(
        ObjectPlacementPreviewMotion::progress_per_second(-2.0),
        17.0
    );
}
