use super::select_bevy_image_asset_path_for_blue_fang_source_image;

#[test]
fn resolves_uppercase_scenario_jpeg_through_the_standard_image_extension() {
    assert_eq!(
        select_bevy_image_asset_path_for_blue_fang_source_image("maps/scenario/cp2scen1.JPG"),
        "maps/scenario/cp2scen1.jpg"
    );
}
