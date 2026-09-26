use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, id, required_number,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::aquatic_habitats::{AquaticRequirement, TankDefinition};

pub(super) fn bind_tank(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.tanks.push(TankDefinition {
        id: id(record.key),
        wall: asset(record, &["wall", "wallDefinition"]),
        floor_material: asset(record, &["floorMaterial", "floor"]),
        water_material: asset(record, &["waterMaterial", "water"]),
        min_depth_cm: required_number(record, &["minDepthCm", "minimumDepthCm"])?,
        max_depth_cm: required_number(record, &["maxDepthCm", "maximumDepthCm"])?,
        capacity_litres_per_cell: required_number(record, &["capacityLitresPerCell"])?,
        filtration_per_day: required_number(record, &["filtrationPerDay"])?,
    });
    Ok(())
}

pub(super) fn bind_aquatic(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output
        .document
        .aquatic_requirements
        .push(AquaticRequirement {
            species: asset(record, &["species"]),
            min_depth_cm: required_number(record, &["minDepthCm", "minimumDepthCm"])?,
            initial_space_m3: required_number(
                record,
                &["initialSpaceM3", "f_RequiredInitialTankSpace"],
            )?,
            additional_space_m3: required_number(
                record,
                &["additionalSpaceM3", "f_RequiredAdditionalTankSpace"],
            )?,
            salinity_permille: array(
                record,
                &["minSalinityPermille", "maxSalinityPermille"],
                [0_u16; 2],
            )?,
            temperature_c: array(record, &["minTemperatureC", "maxTemperatureC"], [0_i16; 2])?,
            water_quality_min: required_number(record, &["waterQualityMin"])?,
            land_fraction_permille: array(
                record,
                &["minLandFractionPermille", "maxLandFractionPermille"],
                [0_u16; 2],
            )?,
        });
    Ok(())
}
