use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, id, required_number,
};
use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::{
    world_definitions::aquatic_habitats::{AquaticRequirement, TankDefinition},
    AssetId,
};

pub(super) fn bind_tank(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let (wall_materials, wall_materials_low) = tank_wall_materials(record.value(&["opaqueMat"]));
    output.document.tanks.push(TankDefinition {
        id: id(record.key),
        wall: asset(record, &["wall", "wallDefinition"]),
        floor_material: AssetId::from_virtual_path(&tank_floor_material_path(
            record.value(&["floorMaterial", "floor"]),
        )),
        water_material: tank_water_material_id(record.value(&["waterMaterial", "water"])),
        min_depth_cm: required_number(record, &["minDepthCm", "minimumDepthCm"])?,
        max_depth_cm: required_number(record, &["maxDepthCm", "maximumDepthCm"])?,
        capacity_litres_per_cell: required_number(record, &["capacityLitresPerCell"])?,
        filtration_per_day: required_number(record, &["filtrationPerDay"])?,
        wall_materials,
        wall_materials_low,
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

fn material_source_path(reference: &str, extension: &str) -> String {
    let reference = crate::source_document::path::AssetPath::new(reference.trim()).key();
    let path = if reference.contains('/') {
        reference
    } else {
        format!("materials/{reference}")
    };
    std::path::Path::new(&path)
        .with_extension(extension)
        .to_string_lossy()
        .into_owned()
}

pub(in crate::world_definitions) fn tank_floor_material_path(reference: Option<&str>) -> String {
    material_source_path(
        reference
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("terrain"),
        "bfmat",
    )
}

pub(super) fn tank_water_material_id(reference: Option<&str>) -> AssetId {
    AssetId::from_virtual_path(&material_source_path(
        reference
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("waterflat"),
        "xml",
    ))
}

pub(in crate::world_definitions) fn tank_wall_material_paths(
    reference: Option<&str>,
) -> [[String; 3]; 2] {
    let reference = reference
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("tankwallglass");
    let path = material_source_path(reference, "bfmat");
    let stem = path.strip_suffix(".bfmat").unwrap_or(&path);
    ["", "_low"].map(|detail| {
        ["nowater", "onewater", "twowater"].map(|water| format!("{stem}_{water}{detail}.bfmat"))
    })
}

pub(super) fn tank_wall_materials(reference: Option<&str>) -> ([AssetId; 3], [AssetId; 3]) {
    let [standard, low] = tank_wall_material_paths(reference)
        .map(|paths| paths.map(|path| AssetId::from_virtual_path(&path)));
    (standard, low)
}
