use super::bind_authored_placement_object;
use crate::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
use crate::source_document::path::AssetPath;
use crate::source_document::resolved_source_record_index::SourceIndex;
use crate::world_definitions::lower::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use std::collections::BTreeMap;

#[test]
fn ground_fit_height_offset_comes_from_the_authored_fit_component() {
    // Shipped pursuitball authors heightOffset 0.75 to rest its centred model on the ground.
    let documents = [
        ("entities/ball.xml", r#"<BFTypedBinder binderType="Ball"><types><entity><toy/></entity></types><shared><BFPhysObj><BFGroundFitComponent heightOffset="0.75" floatOnWater="true"/></BFPhysObj></shared></BFTypedBinder>"#),
        ("entities/rock.xml", r#"<BFTypedBinder binderType="Rock"><types><entity><scenery/></entity></types><shared><BFPhysObj><BFGroundFitComponent/></BFPhysObj></shared></BFTypedBinder>"#),
        ("entities/sign.xml", r#"<BFTypedBinder binderType="Sign"><types><entity><scenery/></entity></types><shared><BFPhysObj/></shared></BFTypedBinder>"#),
    ].map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()).unwrap());
    let index = SourceIndex::build(&documents).unwrap();
    let mut tables = WorldDefinitionLoweringTables::default();
    for key in ["Ball", "Rock", "Sign"] {
        let record = index.find(key).unwrap();
        bind_authored_placement_object(
            &record,
            &[],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &mut tables,
        )
        .unwrap();
    }
    let fit = tables
        .document
        .objects
        .iter()
        .map(|object| (object.terrain_fitted, object.terrain_fit_height_offset_m))
        .collect::<Vec<_>>();
    assert_eq!(fit, [(true, 0.75), (true, 0.0), (false, 0.0)]);
}
