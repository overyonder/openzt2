#[test]
fn rail_camera_uses_the_ui_document_loader_path() {
    assert_eq!(
        super::rail_camera_scene_asset_path("UI/Layout/Credits.xml"),
        "ui/layout/credits.xml.z2ui#railcamera",
    );
}

#[test]
fn rail_models_resolve_extensionless_and_replaced_nif_references() -> std::io::Result<()> {
    use crate::{
        asset_source::{create_bevy_asset_source_for_enabled_z2f_archives, tests::TestDirectory},
        source_document::{
            blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        },
    };

    let directory = TestDirectory::new()?;
    let archive = directory.archive(
        "rail.z2f",
        &[("models/Gate.bfb", b"gate"), ("models/Bin.bfb", b"bin")],
    )?;
    let (_, archives) = create_bevy_asset_source_for_enabled_z2f_archives(vec![archive])?;
    let path = "ui/layout/credits.xml";
    let document = parse_blue_fang_source_document(
        AssetPath::new(path),
        br#"<credits><ZTRailCam time="1"><objects>
            <gate><BFPhysObj><BFSimpleLODComponent modelfile="models/Gate"/></BFPhysObj></gate>
            <bin><BFPhysObj><BFSceneGraphComponent modelfile="models/Bin.nif"/></BFPhysObj></bin>
        </objects></ZTRailCam></credits>"#,
    )
    .unwrap();

    let rail =
        super::lower_rail_camera_documents_to_scene_prefab_documents(&[document], path, &archives)?;
    assert_eq!(rail.len(), 1);
    let dependencies: Vec<_> = rail[0]
        .1
        .dependencies
        .iter()
        .map(|dependency| dependency.asset_path.to_ascii_lowercase())
        .collect();
    assert_eq!(dependencies, ["models/gate.bfb", "models/bin.bfb"]);
    Ok(())
}
