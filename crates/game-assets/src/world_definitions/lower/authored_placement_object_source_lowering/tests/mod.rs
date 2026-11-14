use super::bind_authored_placement_object;
use crate::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
use crate::source_document::path::AssetPath;
use crate::source_document::resolved_source_record_index::SourceIndex;
use crate::world_definitions::lower::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use std::collections::BTreeMap;

#[test]
fn ground_fit_height_offset_comes_from_the_authored_fit_component(
) -> Result<(), Box<dyn std::error::Error>> {
    // Shipped pursuitball authors heightOffset 0.75 to rest its centred model on the ground.
    let documents = [
        ("entities/ball.xml", r#"<BFTypedBinder binderType="Ball"><types><entity><toy/></entity></types><shared><BFPhysObj><BFGroundFitComponent heightOffset="0.75" floatOnWater="true"/></BFPhysObj></shared></BFTypedBinder>"#),
        ("entities/rock.xml", r#"<BFTypedBinder binderType="Rock"><types><entity><scenery/></entity></types><shared><BFPhysObj><BFGroundFitComponent/></BFPhysObj></shared></BFTypedBinder>"#),
        // Shipped wetland trees author floatOnWater with waterOffset -1.75.
        ("entities/tree.xml", r#"<BFTypedBinder binderType="Tree"><types><entity><scenery/></entity></types><shared><BFPhysObj><BFGroundFitComponent floatOnWater="true" waterOffset="-1.75" /></BFPhysObj></shared></BFTypedBinder>"#),
        ("entities/sign.xml", r#"<BFTypedBinder binderType="Sign"><types><entity><scenery/></entity></types><shared><BFPhysObj/></shared></BFTypedBinder>"#),
    ].map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()))
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let index = SourceIndex::build(&documents)?;
    let mut tables = WorldDefinitionLoweringTables::default();
    for key in ["Ball", "Rock", "Tree", "Sign"] {
        let record = index.find(key).ok_or(key)?;
        bind_authored_placement_object(
            &record,
            &[],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &mut tables,
        )?;
    }
    let fit = tables
        .document
        .objects
        .iter()
        .map(|object| {
            (
                object.terrain_fitted,
                object.terrain_fit_height_offset_m,
                object.terrain_fit_water_offset_m,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        fit,
        [
            (true, 0.75, Some(0.0)),
            (true, 0.0, None),
            (true, 0.0, Some(-1.75)),
            (false, 0.0, None)
        ]
    );
    Ok(())
}

#[test]
fn price_comes_from_the_first_economy_component_with_a_cost(
) -> Result<(), Box<dyn std::error::Error>> {
    // A shipped Jeep station takes donations in an economy component without
    // a cost before the one that costs 1000.
    let documents = [(
        "entities/station.xml",
        r#"<BFTypedBinder binderType="Station"><types><entity><station/></entity></types><binder><BFNamedBinder binderName="stationEconomy"><instance><ZTEconomyComponent><ZTTransaction name="donate" cost="10"/></ZTEconomyComponent></instance></BFNamedBinder><BFBinder><instance><ZTEconomyComponent cost="1000"><ZTTransaction name="build" costType="parent"/></ZTEconomyComponent></instance></BFBinder></binder></BFTypedBinder>"#,
    )]
    .map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()))
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let index = SourceIndex::build(&documents)?;
    let mut tables = WorldDefinitionLoweringTables::default();
    bind_authored_placement_object(
        &index.find("Station").ok_or("Station")?,
        &[],
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut tables,
    )?;
    assert_eq!(tables.document.objects[0].price_cents, 100_000);
    Ok(())
}
