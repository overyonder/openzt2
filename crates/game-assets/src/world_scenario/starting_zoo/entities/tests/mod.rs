use super::starting_entity_physical;
use super::{lower_owned_hidden_contents, lower_saved_needs};
use crate::source_document::{
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

#[test]
fn saved_crate_links_use_source_handles_and_needs_keep_authored_values(
) -> Result<(), Box<dyn std::error::Error>> {
    let document = parse_blue_fang_source_document(
        AssetPath::new("starting.xml"),
        br#"
        <BFGEntity><subComponents>
          <BFAIStateVar Name="hunger" Value="53.5"/>
          <BFAIStateVar Name="rest" Value="38.5"/>
          <BFGEntityContainer><slots>
            <BFGEntityContainerSlot ownContents="true" hideContents="true"><entities>
              <entity safehandleID="4685"/>
            </entities></BFGEntityContainerSlot>
          </slots></BFGEntityContainer>
        </subComponents></BFGEntity>"#,
    )?;
    let indexes = std::collections::BTreeMap::from([("4685".to_owned(), 7)]);
    assert_eq!(
        lower_owned_hidden_contents(&document, &document.root, &indexes)?,
        vec![7]
    );
    assert_eq!(
        lower_saved_needs(&document, &document.root)?,
        vec![
            (openzt2_game_data::species::NeedKind::Hunger, 465 * 65_536),
            (openzt2_game_data::species::NeedKind::Rest, 615 * 65_536),
        ]
    );
    assert!(lower_owned_hidden_contents(
        &document,
        &document.root,
        &std::collections::BTreeMap::new()
    )
    .is_err());
    Ok(())
}
