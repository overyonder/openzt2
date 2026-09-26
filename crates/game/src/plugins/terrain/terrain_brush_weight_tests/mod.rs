use super::{
    terrain_brush_sample_mutation::calculate_terrain_brush_weight_with_falloff,
    terrain_brush_types::TerrainBrushFalloff,
};

#[test]
fn linear_terrain_brush_falloff_is_finite_and_bounded() {
    let linear = TerrainBrushFalloff::Linear;
    assert_eq!(
        calculate_terrain_brush_weight_with_falloff(0.0, 4.0, linear),
        1.0
    );
    assert_eq!(
        calculate_terrain_brush_weight_with_falloff(2.0, 4.0, linear),
        0.5
    );
    assert_eq!(
        calculate_terrain_brush_weight_with_falloff(4.0, 4.0, linear),
        0.0
    );
    assert_eq!(
        calculate_terrain_brush_weight_with_falloff(8.0, 4.0, linear),
        0.0
    );
}

#[test]
fn native_cosine_terrain_height_brush_has_double_strength_at_the_centre() {
    let cosine = TerrainBrushFalloff::Cosine;
    assert_eq!(
        calculate_terrain_brush_weight_with_falloff(0.0, 4.0, cosine),
        2.0
    );
    assert!((calculate_terrain_brush_weight_with_falloff(2.0, 4.0, cosine) - 1.0).abs() < 1e-6);
    assert!(calculate_terrain_brush_weight_with_falloff(4.0, 4.0, cosine).abs() < 1e-6);
    assert_eq!(
        calculate_terrain_brush_weight_with_falloff(8.0, 4.0, cosine),
        0.0
    );
}
