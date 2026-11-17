use super::super::object_and_placeable_source_lowering::bind_object;
use super::super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use crate::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
use crate::source_document::path::AssetPath;
use crate::source_document::resolved_source_record_index::SourceIndex;

#[test]
fn inherited_trick_component_distinguishes_show_capability_from_plain_food() {
    let documents = [
        ("entities/animal.xml", r#"<BFTypedBinder binderType="Animal" abstract="true"><instance><ZTAITrickComponent/></instance></BFTypedBinder>"#),
        ("entities/performer.xml", r#"<BFTypedBinder binderType="Performer" kind="animal"><types><entity><Animal><Performer/></Animal></entity></types></BFTypedBinder>"#),
        ("entities/food.xml", r#"<BFTypedBinder binderType="FoodDish" kind="food"><instance><BFAIEntityDataInstance f_FoodLevel="0"/></instance></BFTypedBinder>"#),
    ].map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()).unwrap());
    let index = SourceIndex::build(&documents).unwrap();
    let mut tables = WorldDefinitionLoweringTables::default();
    bind_object(&index.find("Performer").unwrap(), &mut tables).unwrap();
    bind_object(&index.find("FoodDish").unwrap(), &mut tables).unwrap();
    assert!(tables.document.objects[0].supports_show_tricks);
    assert!(!tables.document.objects[1].supports_show_tricks);
}

#[test]
fn building_strength_is_inherited_from_the_abstract_type_binder() {
    let documents = [
        ("entities/bathroomsmall.xml", r#"<BFTypedBinder binderType="bathroomsmall" abstract="true"><types><entity><building><bathroomsmall/></building></entity></types><shared><BFAIEntityDataShared f_BuildingStrength="20"/></shared></BFTypedBinder>"#),
        ("entities/bathroomsmall_xt.xml", r#"<BFTypedBinder binderType="bathroomsmall_xt" kind="facility"><types><entity><building><bathroomsmall><bathroomsmall_xt/></bathroomsmall></building></entity></types><shared><ZTEntityDestructionData explosionEffect="e" explosionSound="s" rubbleBinderName="r" entityRuinBinderName="rubble_xt" secsToRemove="0.3f"/></shared></BFTypedBinder>"#),
    ].map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()).unwrap());
    let index = SourceIndex::build(&documents).unwrap();
    let mut tables = WorldDefinitionLoweringTables::default();
    bind_object(&index.find("bathroomsmall_xt").unwrap(), &mut tables).unwrap();
    let destruction = tables.document.objects[0].destruction.as_ref().unwrap();
    assert_eq!(destruction.strength, Some(20));
}
