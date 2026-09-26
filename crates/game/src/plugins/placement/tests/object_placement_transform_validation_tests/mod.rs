use bevy::prelude::*;
use openzt2_game_data::world_definitions::object_placement::{
    FootprintCell, FootprintCellFlags, PlacementConstraints,
};

use super::super::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;

use super::super::object_placement_validation::{
    calculate_authored_object_placement_eighth_turns, calculate_object_placement_footprint_origin,
    calculate_terrain_slope_permille, snap_object_placement_transform_to_authored_grid,
};
use super::create_object_placeable_definition_test_fixture;

#[test]
fn asymmetric_authored_footprint_cells_follow_bevy_model_rotation_in_both_world_quadrants() {
    // Winning x301 entities/objects/buildings/ai/restaurant_fancy_df.xml:
    // cfootprint width=5 height=3. The source lowerer centers its pivot at
    // half those dimensions, in centimetres.
    let mut definition =
        create_object_placeable_definition_test_fixture(PlacementConstraints::EMPTY);
    definition.pivot_cm = [250, 150];
    definition.footprint = (0..3)
        .flat_map(|z| {
            (0..5).map(move |x| FootprintCell {
                offset: [x, z],
                flags: FootprintCellFlags::OCCUPIED,
            })
        })
        .collect();
    let pivot = Vec3::new(2.5, 0.0, 1.5);
    let mut scratch = Vec::new();
    for position in [Vec3::new(12.3, 0.0, 8.7), Vec3::new(-12.3, 0.0, -8.7)] {
        for quarter in 0..4 {
            let transform = snap_object_placement_transform_to_authored_grid(
                &definition,
                Transform::from_translation(position).with_rotation(Quat::from_rotation_y(
                    quarter as f32 * std::f32::consts::FRAC_PI_2,
                )),
            )
            .unwrap();
            let actual = collect_occupied_placement_cells_for_transform(
                &definition,
                &transform,
                &mut scratch,
            )
            .unwrap();
            // Independently transform cell centers with the rendered model's
            // Bevy transform; floor, rather than truncation, selects cells
            // correctly in negative world coordinates and reversed axes.
            let mut expected = definition
                .footprint
                .iter()
                .map(|cell| {
                    let local = Vec3::new(
                        f32::from(cell.offset[0]) + 0.5,
                        0.0,
                        f32::from(cell.offset[1]) + 0.5,
                    ) - pivot;
                    transform.transform_point(local).xz().floor().as_ivec2()
                })
                .collect::<Vec<_>>();
            expected.sort_unstable_by_key(|cell| (cell.y, cell.x));
            assert_eq!(actual, expected, "position={position:?} quarter={quarter}");
        }
    }
}

#[test]
fn authored_pivot_offsets_the_footprint_origin_after_rotation() {
    let mut fixture = create_object_placeable_definition_test_fixture(PlacementConstraints::EMPTY);
    fixture.pivot_cm = [100, 0];
    let object_placeable_definition = &fixture;

    let transform = Transform::from_xyz(4.0, 0.0, 5.0);
    assert_eq!(
        calculate_object_placement_footprint_origin(object_placeable_definition, &transform, 0),
        Some(IVec2::new(3, 5))
    );
    assert_eq!(
        calculate_object_placement_footprint_origin(object_placeable_definition, &transform, 2),
        Some(IVec2::new(4, 6))
    );
}

#[test]
fn authored_diagonal_rotation_uses_the_diagonal_pivot() {
    let mut fixture = create_object_placeable_definition_test_fixture(PlacementConstraints::EMPTY);
    fixture.rotation_increment_degrees = 45;
    fixture.pivot_cm = [100, 0];
    fixture.diagonal_pivot_cm = [150, 150];
    let object_placeable_definition = &fixture;

    let transform = Transform::from_xyz(4.0, 0.0, 5.0)
        .with_rotation(Quat::from_rotation_y(core::f32::consts::FRAC_PI_4));
    assert_eq!(
        calculate_authored_object_placement_eighth_turns(object_placeable_definition, &transform),
        Some(1)
    );
    assert_eq!(
        calculate_object_placement_footprint_origin(object_placeable_definition, &transform, 1),
        Some(IVec2::new(3, 4))
    );
}

#[test]
fn placement_snap_uses_the_same_authored_origin_as_validation() {
    let mut fixture = create_object_placeable_definition_test_fixture(PlacementConstraints::EMPTY);
    fixture.pivot_cm = [50, 50];
    let object_placeable_definition = &fixture;

    let raw = Transform::from_xyz(4.31, 2.0, 5.82);
    let snapped =
        snap_object_placement_transform_to_authored_grid(object_placeable_definition, raw)
            .expect("valid authored pose");
    assert_eq!(snapped.translation, Vec3::new(4.5, 2.0, 5.5));
    assert_eq!(
        calculate_object_placement_footprint_origin(object_placeable_definition, &snapped, 0),
        Some(IVec2::new(4, 5))
    );
}

#[test]
fn authored_rotation_rejects_unsnapped_and_tilted_transforms() {
    let object_placeable_definition =
        &create_object_placeable_definition_test_fixture(PlacementConstraints::EMPTY);

    let quarter = Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
    assert_eq!(
        calculate_authored_object_placement_eighth_turns(object_placeable_definition, &quarter),
        Some(2)
    );
    let unsnapped = Transform::from_rotation(Quat::from_rotation_y(0.3));
    assert_eq!(
        calculate_authored_object_placement_eighth_turns(object_placeable_definition, &unsnapped),
        None
    );
    let tilted = Transform::from_rotation(Quat::from_rotation_x(0.1));
    assert_eq!(
        calculate_authored_object_placement_eighth_turns(object_placeable_definition, &tilted),
        None
    );
}

#[test]
fn slope_conversion_is_stable_at_boundaries() {
    assert_eq!(calculate_terrain_slope_permille(Vec3::Y), 0);
    assert_eq!(
        calculate_terrain_slope_permille(Vec3::new(0.25, 1.0, 0.0)),
        250
    );
    assert_eq!(
        calculate_terrain_slope_permille(Vec3::new(0.0, 0.0, 1.0)),
        u32::MAX
    );
}
