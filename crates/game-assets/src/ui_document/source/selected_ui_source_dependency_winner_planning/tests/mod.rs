use crate::asset_source::{
    create_bevy_asset_source_for_enabled_z2f_archives, tests::TestDirectory,
};
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[test]
fn selects_ui_images_with_spaces_in_the_archived_filename() -> std::io::Result<()> {
    let directory = TestDirectory::new()?;
    let archive = directory.archive(
        "credits.z2f",
        &[("ui/credits/logos/Sea Fan.dds", b"texture")],
    )?;
    let (_, archives) = create_bevy_asset_source_for_enabled_z2f_archives(vec![archive])?;
    let document = parse_blue_fang_source_document(
        AssetPath::new("ui/template/backs/credits.xml"),
        br#"<UIAspect image="ui/credits/logos/Sea Fan.dds"/>"#,
    )
    .unwrap();

    let dependencies = super::plan_selected_ui_source_dependency_winners_and_image_dimensions(
        &[document],
        &archives,
    );
    assert_eq!(
        dependencies
            .images
            .get("*\0ui/credits/logos/sea fan.dds")
            .map(String::as_str),
        Some("ui/credits/logos/Sea Fan.dds")
    );
    Ok(())
}
