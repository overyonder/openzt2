use super::super::object_detach_action_source_lowering::lower_object_detach_actions;
use super::super::world_definition_source_value_reading_and_conversion::id;
use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
use crate::assets::source_document::path::AssetPath;
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDetachDestination;

#[test]
fn inherited_item_detach_rules_keep_inventory_distinct_from_destruction() {
    let documents = [
        ("entities/item.xml", br#"<BFTypedBinder binderType="item" abstract="true"><shared><BFGDetachInfo><detachActionTable><killitem destination="kill"/><inventory destination="inventory"/><dropitem destination="drop"/></detachActionTable></BFGDetachInfo></shared></BFTypedBinder>"#.as_slice()),
        ("entities/cup.xml", br#"<BFTypedBinder binderType="Cup"><types><entity><item><Cup/></item></entity></types></BFTypedBinder>"#.as_slice()),
    ].map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source).unwrap());
    let index = SourceIndex::build(&documents).unwrap();
    let rules = lower_object_detach_actions(&index.find("Cup").unwrap());
    let destination = |name| {
        rules
            .iter()
            .find(|rule| rule.name == id(name))
            .unwrap()
            .destination
    };
    assert_eq!(destination("killitem"), WorldObjectDetachDestination::Kill);
    assert_eq!(destination("dropitem"), WorldObjectDetachDestination::Drop);
    assert_eq!(
        destination("inventory"),
        WorldObjectDetachDestination::Container(id("inventory"))
    );
}
