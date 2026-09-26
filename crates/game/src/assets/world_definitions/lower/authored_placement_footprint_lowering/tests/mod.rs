use super::super::authored_placement_footprint_lowering::{
    authored_ground_paths_block_placement, authored_headroom_policy, bind_authored_placeable,
};
use super::super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
use crate::assets::source_document::path::AssetPath;
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use openzt2_game_data::world_definitions::object_placement::FootprintCell;

#[test]
fn placement_headroom_preserves_inheritance_fractional_metres_and_native_defaults() {
    let documents = [
        ("entities/base.xml", r#"<BFTypedBinder binderType="Base" abstract="true"><instance><ZTPlacementData minimumHeadroom="1.2345" applyHeightModifier="false"/></instance></BFTypedBinder>"#),
        ("entities/child.xml", r#"<BFTypedBinder binderType="Child"><types><entity><Base><Child/></Base></entity></types></BFTypedBinder>"#),
        ("entities/default.xml", r#"<BFTypedBinder binderType="Default"><instance><ZTPlacementData/></instance></BFTypedBinder>"#),
        ("entities/sentinel.xml", r#"<BFTypedBinder binderType="Sentinel"><instance><ZTPlacementData minimumHeadroom="-1"/></instance></BFTypedBinder>"#),
    ].map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()).unwrap());
    let index = SourceIndex::build(&documents).unwrap();
    assert_eq!(
        authored_headroom_policy(&index.find("Child").unwrap()).unwrap(),
        (1.2345, false)
    );
    assert_eq!(
        authored_headroom_policy(&index.find("Default").unwrap()).unwrap(),
        (0.0, true)
    );
    assert_eq!(
        authored_headroom_policy(&index.find("Sentinel").unwrap()).unwrap(),
        (0.0, true)
    );
}

#[test]
fn zero_cardinal_footprint_occupies_the_cell_containing_the_object() {
    // Shipped trashcan_df authors cfootprint 0x0 and dfootprint 1x1.
    let documents = [(
        "entities/trashcan.xml",
        r#"<BFTypedBinder binderType="Trashcan"><shared><ZTPlacementData><cfootprint width="0" height="0"/><dfootprint width="1" height="1"/></ZTPlacementData></shared></BFTypedBinder>"#,
    )]
    .map(|(path, source)| {
        parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()).unwrap()
    });
    let index = SourceIndex::build(&documents).unwrap();
    let mut tables = WorldDefinitionLoweringTables::default();
    bind_authored_placeable(
        &index.find("Trashcan").unwrap(),
        None,
        false,
        0,
        &mut tables,
    )
    .unwrap();
    let placeable = &tables.document.placeables[0];
    let offsets =
        |cells: &[FootprintCell]| cells.iter().map(|cell| cell.offset).collect::<Vec<_>>();
    assert_eq!(offsets(&placeable.footprint), [[0, 0]]);
    assert_eq!(placeable.pivot_cm, [50, 50]);
    assert_eq!(offsets(&placeable.diagonal_footprint), [[0, 0]]);
    assert_eq!(placeable.diagonal_pivot_cm, [50, 50]);
}

#[test]
fn ground_paths_block_only_grid_snapped_objects_whose_stomp_rules_prevent_paths() {
    let documents = [
        ("entities/entity.xml", r#"<BFTypedBinder binderType="entity" abstract="true"><shared><ZTPlacementData><stompData><allow><path/></allow><prevent/></stompData></ZTPlacementData></shared></BFTypedBinder>"#),
        ("entities/scenery.xml", r#"<BFTypedBinder binderType="scenery" abstract="true"><types><entity><scenery/></entity></types><shared><ZTPlacementData gridSnap="true"><stompData><delete><grass/></delete><prevent/><allow/></stompData></ZTPlacementData></shared></BFTypedBinder>"#),
        ("entities/bin.xml", r#"<BFTypedBinder binderType="bin"><types><entity><scenery><bin/></scenery></entity></types></BFTypedBinder>"#),
        ("entities/toy.xml", r#"<BFTypedBinder binderType="toy"><types><entity><toy/></entity></types><shared><ZTPlacementData gridSnap="true"/></shared></BFTypedBinder>"#),
        ("entities/egg.xml", r#"<BFTypedBinder binderType="egg"><types><entity><egg/></entity></types><shared><ZTPlacementData><stompData><prevent><path/></prevent></stompData></ZTPlacementData></shared></BFTypedBinder>"#),
    ].map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()).unwrap());
    let index = SourceIndex::build(&documents).unwrap();
    let blocks =
        |key: &str| authored_ground_paths_block_placement(&index.find(key).unwrap()).unwrap();
    assert!(blocks("bin"));
    assert!(!blocks("toy"));
    assert!(!blocks("egg"));
}
