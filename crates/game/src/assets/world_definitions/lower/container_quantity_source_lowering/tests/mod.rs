use super::super::container_quantity_source_lowering::lower_authored_container_quantity;
use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocument;
use crate::assets::source_document::path::AssetPath;
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use openzt2_game_data::world_definitions::world_objects::WorldObjectContainerContent;

fn parse(path: &str, source: &str) -> OrderedSourceDocument {
    parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes())
        .expect("valid resolved-loader fixture document")
}

fn lowered_record<'index, 'document>(
    index: &'index SourceIndex<'document>,
    key: &str,
) -> openzt2_game_data::world_definitions::world_objects::WorldObjectContainerQuantityDefinition {
    lower_authored_container_quantity(&index.find(key).expect("fixture record"))
        .expect("container quantity lowering succeeds")
        .expect("fixture authors FoodLevel")
}

#[test]
fn inherited_food_dish_water_typing_lowers_as_drink() {
    let documents = [
        parse(
            "entities/objects/food/ai/FoodDish.xml",
            r#"
                <BFTypedBinder binderType="FoodDish" abstract="true">
                    <shared><BFAIEntityDataShared b_Water="true"/></shared>
                </BFTypedBinder>
            "#,
        ),
        parse(
            "entities/objects/food/ai/TestDish.xml",
            r#"
                <BFTypedBinder binderType="TestDish">
                    <types><entity><food><FoodDish><TestDish/></FoodDish></food></entity></types>
                    <instance><BFAIEntityDataInstance f_FoodLevel="25"/></instance>
                </BFTypedBinder>
            "#,
        ),
    ];
    let index = SourceIndex::build(&documents).expect("resolved source index");
    let quantity = lowered_record(&index, "TestDish");
    assert_eq!(quantity.content, WorldObjectContainerContent::Drink);
    assert_eq!(quantity.initial_q16, 25 << 16);
}

#[test]
fn direct_water_value_overrides_inherited_food_dish_typing() {
    let documents = [
        parse(
            "entities/objects/food/ai/FoodDish.xml",
            r#"
                <BFTypedBinder binderType="FoodDish" abstract="true">
                    <shared><BFAIEntityDataShared b_Water="true"/></shared>
                </BFTypedBinder>
            "#,
        ),
        parse(
            "entities/objects/food/ai/TestMeat.xml",
            r#"
                <BFTypedBinder binderType="TestMeat">
                    <types><entity><food><FoodDish><TestMeat/></FoodDish></food></entity></types>
                    <shared><BFAIEntityDataShared b_Water="false"/></shared>
                    <instance><BFAIEntityDataInstance f_FoodLevel="25"/></instance>
                </BFTypedBinder>
            "#,
        ),
    ];
    let index = SourceIndex::build(&documents).expect("resolved source index");
    assert_eq!(
        lowered_record(&index, "TestMeat").content,
        WorldObjectContainerContent::Food
    );
}

#[test]
fn explicit_zero_is_preserved_and_missing_food_level_stays_absent() {
    let documents = [
        parse(
            "entities/objects/food/ai/FoodDish.xml",
            r#"
                <BFTypedBinder binderType="FoodDish" abstract="true">
                    <shared><BFAIEntityDataShared b_Water="false"/></shared>
                </BFTypedBinder>
            "#,
        ),
        parse(
            "entities/objects/food/ai/ZeroDish.xml",
            r#"
                <BFTypedBinder binderType="ZeroDish">
                    <types><entity><food><FoodDish><ZeroDish/></FoodDish></food></entity></types>
                    <instance><BFAIEntityDataInstance f_FoodLevel="0"/></instance>
                </BFTypedBinder>
            "#,
        ),
        parse(
            "entities/objects/food/ai/UnquantifiedDish.xml",
            r#"
                <BFTypedBinder binderType="UnquantifiedDish">
                    <types><entity><food><FoodDish><UnquantifiedDish/></FoodDish></food></entity></types>
                </BFTypedBinder>
            "#,
        ),
    ];
    let index = SourceIndex::build(&documents).expect("resolved source index");
    assert_eq!(lowered_record(&index, "ZeroDish").initial_q16, 0);
    assert!(lower_authored_container_quantity(
        &index.find("UnquantifiedDish").expect("fixture record")
    )
    .expect("missing quantity is valid absence")
    .is_none());
}

#[test]
fn generic_object_record_uses_the_same_container_quantity_lowering() {
    let document = parse(
        "entities/objects/generic/ai/GenericDish.xml",
        r#"
            <BFTypedBinder binderType="GenericDish" kind="food">
                <shared><BFAIEntityDataShared b_Water="false"/></shared>
                <instance><BFAIEntityDataInstance f_FoodLevel="25"/></instance>
            </BFTypedBinder>
        "#,
    );
    let index = SourceIndex::build([&document]).expect("resolved source index");
    let mut tables = super::super::WorldDefinitionLoweringTables::default();
    super::super::object_and_placeable_source_lowering::bind_object(
        &index.find("GenericDish").expect("fixture record"),
        &mut tables,
    )
    .expect("generic object lowering");
    let quantity = tables.document.objects[0]
        .container_quantity
        .expect("generic object quantity");
    assert_eq!(quantity.content, WorldObjectContainerContent::Food);
    assert_eq!(quantity.initial_q16, 25 << 16);
}
