use super::starting_entity_physical;
use crate::assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[test]
fn starting_entity_without_physical_components_has_no_physical_selection(
) -> Result<(), Box<dyn std::error::Error>> {
    let document = parse_blue_fang_source_document(
        AssetPath::new("empty.xml"),
        b"<subcomponents><BFSceneGraph/></subcomponents>",
    )?;
    assert!(starting_entity_physical(&document.root).is_none());
    Ok(())
}
