use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, id, required_number,
};
use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::{
    AssetId,
    world_definitions::aquatic_habitats::{AquaticRequirement, TankDefinition},
};

pub(super) fn bind_tank(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let TankMaterials {
        floor,
        water,
        wall,
        wall_low,
    } = tank_materials(
        output,
        record.value(&["floorMaterial", "floor"]),
        record.value(&["waterMaterial", "water"]),
        record.value(&["opaqueMat"]),
    );
    output.document.tanks.push(TankDefinition {
        id: id(record.key),
        wall: asset(record, &["wall", "wallDefinition"]),
        floor_material: floor,
        water_material: water,
        min_depth_cm: required_number(record, &["minDepthCm", "minimumDepthCm"])?,
        max_depth_cm: required_number(record, &["maxDepthCm", "maximumDepthCm"])?,
        capacity_litres_per_cell: required_number(record, &["capacityLitresPerCell"])?,
        filtration_per_day: required_number(record, &["filtrationPerDay"])?,
        wall_materials: wall,
        wall_materials_low: wall_low,
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

fn tank_floor_material_path(reference: Option<&str>) -> String {
    material_source_path(
        reference
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("terrain"),
        "bfmat",
    )
}

fn tank_water_material_path(reference: Option<&str>) -> String {
    material_source_path(
        reference
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("waterflat"),
        "xml",
    )
}

fn tank_wall_material_paths(reference: Option<&str>) -> [[String; 3]; 2] {
    let reference = reference
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("tankwallglass");
    let path = material_source_path(reference, "bfmat");
    let stem = path.strip_suffix(".bfmat").unwrap_or(&path);
    ["", "_low"].map(|detail| {
        ["nowater", "onewater", "twowater"].map(|water| format!("{stem}_{water}{detail}.bfmat"))
    })
}

pub(super) struct TankMaterials {
    pub(super) floor: AssetId,
    pub(super) water: AssetId,
    pub(super) wall: [AssetId; 3],
    pub(super) wall_low: [AssetId; 3],
}

pub(super) fn tank_materials(
    output: &mut WorldDefinitionLoweringTables,
    floor: Option<&str>,
    water: Option<&str>,
    wall: Option<&str>,
) -> TankMaterials {
    let path_id = |path: &String| AssetId::from_virtual_path(path);
    let floor = tank_floor_material_path(floor);
    let water = tank_water_material_path(water);
    let [wall, wall_low] = tank_wall_material_paths(wall);
    let materials = TankMaterials {
        floor: path_id(&floor),
        water: path_id(&water),
        wall: wall.each_ref().map(path_id),
        wall_low: wall_low.each_ref().map(path_id),
    };
    output
        .tank_material_paths
        .extend([floor, water].into_iter().chain(wall).chain(wall_low));
    materials
}
