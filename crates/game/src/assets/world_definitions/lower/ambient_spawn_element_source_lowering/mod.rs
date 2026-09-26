use super::ambient_class_source_vocabulary::ambient_class;
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    asset_list, element_array, element_asset, id, required_element, required_element_number,
    simple_error,
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::BindError;
use openzt2_game_data::world_definitions::environment::AmbientSpawnDefinition;
use openzt2_game_data::AssetId;

pub(super) fn bind_ambient_element(
    environment: AssetId,
    element: &'_ OrderedSourceDocumentNode,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let radius_cm = element_array(element, &["minimumRadiusCm", "maximumRadiusCm"], [0_u32; 2])?;
    if radius_cm != [0, 0] {
        return Err(simple_error("ambient spawn radius requires an origin"));
    }
    let effect = element_asset(element, &["effect", "particle"]);
    if effect != AssetId::default() {
        return Err(simple_error(
            "ambient effect reference does not identify both the effect asset and its emitter",
        ));
    }
    let prefabs = element
        .attribute_named_any(&["prefabs", "entities"])
        .map(asset_list)
        .unwrap_or_default();
    let biomes = element
        .attribute_named_any(&["biomes"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.ambient_spawns.push(AmbientSpawnDefinition {
        id: id(required_element(element, &["id", "name"])?),
        environment,
        prefabs,
        biomes,
        class: ambient_class(
            element
                .attribute_named_any(&["class", "ambientClass"])
                .unwrap_or("ground"),
        )?,
        day_fraction: element_array(
            element,
            &["minimumDayFraction", "maximumDayFraction"],
            [0_u16, u16::MAX],
        )?,
        population: element_array(
            element,
            &["minimumPopulation", "maximumPopulation"],
            [0_u16; 2],
        )?,
        spawn_interval_ticks: element_array(
            element,
            &["minimumSpawnIntervalTicks", "maximumSpawnIntervalTicks"],
            [0_u32; 2],
        )?,
        radius_cm,
        lifetime_ticks: element_array(
            element,
            &["minimumLifetimeTicks", "maximumLifetimeTicks"],
            [0_u32; 2],
        )?,
        probability: required_element_number(element, &["probability"])?,
        audio: element_asset(element, &["audio", "sound"]),
        effect_asset: AssetId::default(),
        effect_emitter: AssetId::default(),
    });
    Ok(())
}
