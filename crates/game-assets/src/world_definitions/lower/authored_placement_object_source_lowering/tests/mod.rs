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
fn a_water_filter_reads_its_maintenance_schedule_thresholds(
) -> Result<(), Box<dyn std::error::Error>> {
    // As the shipped Water_Filter authors them.
    let documents = [(
        "entities/filter.xml",
        r#"<BFTypedBinder binderType="Filter"><types><entity><building><Filter/></building></entity></types><binder><BFBinder><instance><ZTAIWaterFilterComponent cleanupInterval="2.0" cleanupRate="750.0" maximumDirtCapacity="75000.0"><maintenanceThresholds><preemptive threshold="20" cancelThreshold="10"/><regular threshold="50" cancelThreshold="10"/><occasional threshold="80" cancelThreshold="10"/></maintenanceThresholds></ZTAIWaterFilterComponent></instance></BFBinder></binder></BFTypedBinder>"#,
    )]
    .map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()))
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let index = SourceIndex::build(&documents)?;
    let mut tables = WorldDefinitionLoweringTables::default();
    bind_authored_placement_object(
        &index.find("Filter").ok_or("Filter")?,
        &[],
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut tables,
    )?;
    assert_eq!(
        tables.document.objects[0]
            .water_filter
            .and_then(|filter| filter.maintenance_thresholds),
        Some([[20.0, 10.0], [50.0, 10.0], [80.0, 10.0]])
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

#[test]
fn a_building_inherits_its_strength_and_docks_from_its_type_binders(
) -> Result<(), Box<dyn std::error::Error>> {
    // Shipped hot dog stands get strength 10 from the abstract Foodstand binder.
    let documents = [
        ("entities/foodstand.xml", r#"<BFTypedBinder binderType="Foodstand" abstract="true"><types><entity><building><Foodstand/></building></entity></types><shared><BFAIEntityDataShared f_BuildingStrength="10" p_TalkOffset="0 -1 180"/><BFSharedPhysVars><virtualNodes><Dock_Rampage1 y="-1.5"/></virtualNodes></BFSharedPhysVars></shared></BFTypedBinder>"#),
        ("entities/building.xml", r#"<BFTypedBinder binderType="building" abstract="true"><types><entity><building/></entity></types><shared><BFSharedPhysVars><virtualNodes><Dock_Rampage1 y="-4" yaw="180"/><Dock_Rampage2 x="-4" yaw="-90"/></virtualNodes></BFSharedPhysVars></shared></BFTypedBinder>"#),
        ("entities/hotdog.xml", r#"<BFTypedBinder binderType="Hotdog"><types><entity><building><Foodstand><Hotdog/></Foodstand></building></entity></types><shared><ZTEntityDestructionData explosionEffect="e" explosionSound="s" rubbleBinderName="r" entityRuinBinderName="rubble_xt" secsToRemove="0.3f"/></shared></BFTypedBinder>"#),
    ].map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()))
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let index = SourceIndex::build(&documents)?;
    let mut tables = WorldDefinitionLoweringTables::default();
    bind_authored_placement_object(
        &index.find("Hotdog").ok_or("Hotdog")?,
        &[],
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut tables,
    )?;
    let destruction = tables.document.objects[0]
        .destruction
        .as_ref()
        .ok_or("no destruction")?;
    assert_eq!(destruction.strength, Some(10));
    assert_eq!(destruction.effect_path, "particle/e.psys");
    // The stand moves the building's dock closer but keeps its yaw, which
    // turns the Bevy way: the dock left of the building faces it, along +X.
    let nodes = &tables.document.objects[0].virtual_nodes;
    assert_eq!(nodes.len(), 3);
    // A position attribute, as a guest's p_TalkOffset, is a node too.
    assert_eq!(
        (
            nodes[2].name.as_str(),
            nodes[2].translation_m,
            nodes[2].yaw_degrees
        ),
        ("p_TalkOffset", [0.0, 0.0, 1.0], -180.0)
    );
    assert_eq!(nodes[0].name, "Dock_Rampage1");
    assert_eq!(
        (nodes[0].translation_m, nodes[0].yaw_degrees),
        ([0.0, 0.0, 1.5], -180.0)
    );
    assert_eq!(
        (nodes[1].translation_m, nodes[1].yaw_degrees),
        ([-4.0, 0.0, -0.0], 90.0)
    );
    Ok(())
}

#[test]
fn a_ruin_or_stump_can_be_put_down_although_it_has_no_placement_data(
) -> Result<(), Box<dyn std::error::Error>> {
    // As the shipped rubble_xt_1 a smashed hot dog stand gives way to, and a
    // smashed tree's stump.
    let documents = [(
        "entities/rubble_xt_1.xml",
        r#"<BFTypedBinder binderType="rubble_xt_1"><types><entity><building><Rubble><rubble_xt><rubble_xt_1/></rubble_xt></Rubble></building></entity></types><shared/></BFTypedBinder>"#,
    ), (
        "entities/stump_1.xml",
        r#"<BFTypedBinder binderType="ThinBrownLarge_Stump_1"><types><entity><tree><stump><ThinBrownLarge_Stump><ThinBrownLarge_Stump_1/></ThinBrownLarge_Stump></stump></tree></entity></types><shared/></BFTypedBinder>"#,
    )]
    .map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()))
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let index = SourceIndex::build(&documents)?;
    let mut tables = WorldDefinitionLoweringTables::default();
    for key in ["rubble_xt_1", "ThinBrownLarge_Stump_1"] {
        bind_authored_placement_object(
            &index.find(key).ok_or(key)?,
            &[],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &mut tables,
        )?;
    }
    assert_eq!(tables.document.placeables.len(), 2);
    Ok(())
}
