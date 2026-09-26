use super::authored_zoopedia_subject_from_world_object_record;
use super::object_facility_staff_and_guest_source_vocabulary::{
    entrance_purpose, world_object_information_view_class, world_object_kind,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_flag_vocabulary::{
    affordance, footprint_flag, object_tag, placement_flag,
};
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, bool_or, element_array, element_bool, element_number, flags, id,
    money_cents_i64_or, number_or, parse_f32_triplet, required, required_element, seconds_ns,
};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::object_placement::{
    EntranceDefinition, FootprintCell, FootprintCellFlags, PlaceableDefinition,
    PlacementConstraints,
};
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectAffordanceFlags, WorldObjectBiomeAutomaticPlacementClass, WorldObjectDefinition,
    WorldObjectDestructionDefinition, WorldObjectPropertyFlags,
    WorldObjectRealPhysicsWaterImpactDefinition,
};
use openzt2_game_data::AssetId;

#[cfg(test)]
mod tests;

pub(super) fn bind_object(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let destruction = record
        .descendant_named("ZTEntityDestructionData")
        .map(|data| {
            let effect = required_element(&data, &["explosionEffect"])?;
            let sound = required_element(&data, &["explosionSound"])?;
            let rubble = required_element(&data, &["rubbleBinderName"])?;
            let ruin = required_element(&data, &["entityRuinBinderName"])?;
            let delay_seconds: f64 =
                parse_blue_fang_source_numeric_lexeme(required_element(&data, &["secsToRemove"])?)
                    .ok_or_else(|| BindError::record(record, "invalid destruction secsToRemove"))?;
            let delay_ns = seconds_ns(delay_seconds, record)?;
            Ok(WorldObjectDestructionDefinition {
                effect: id(&format!("particle/{effect}")),
                audio_cue: AssetId::from_key(&format!("audio-cue:{sound}")),
                rubble: id(rubble),
                ruin: id(ruin),
                rubble_count: element_number(&data, &["numParticles"], 0)?,
                delay_ns,
                apply_physics: element_bool(&data, &["applyPhysToRubble"], false)?,
                force_mps: [
                    element_number(&data, &["forceToRubbleMin"], 0.0)?,
                    element_number(&data, &["forceToRubbleMax"], 0.0)?,
                ],
                pitch_degrees: element_number(&data, &["rubblePitchAngle"], 0.0)?,
                start_height_cm: (element_number::<f32>(&data, &["rubbleStartHeight"], 0.0)?
                    * 100.0)
                    .round() as i32,
            })
        })
        .transpose()?;
    let biome_data = record.descendant_named("BFGBiomeData");
    let biomes = biome_data
        .into_iter()
        .flat_map(|data| data.element_children())
        .map(|biome| id(biome.name.as_str()))
        .collect();
    let placement = record.descendant_named("ZTPlacementData");
    let real_physics_water_impact = record
        .descendant_named("BFRealPhysicsComponent")
        .map(|physics| {
            Ok(WorldObjectRealPhysicsWaterImpactDefinition {
                shape_radius_m: physics
                    .attribute_named_any(&["shapeRadius"])
                    .map(|value| {
                        parse_blue_fang_source_numeric_lexeme(value)
                            .ok_or_else(|| BindError::record(record, "invalid shapeRadius"))
                    })
                    .transpose()?
                    .unwrap_or(1.0),
                maximum_splash_speed_mps: physics
                    .attribute_named_any(&["maxSplashSpeed"])
                    .map(|value| {
                        parse_blue_fang_source_numeric_lexeme(value)
                            .ok_or_else(|| BindError::record(record, "invalid maxSplashSpeed"))
                    })
                    .transpose()?
                    .unwrap_or(1.0),
                maximum_splash_strength: physics
                    .attribute_named_any(&["maxSplashStrength"])
                    .map(|value| {
                        parse_blue_fang_source_numeric_lexeme(value)
                            .ok_or_else(|| BindError::record(record, "invalid maxSplashStrength"))
                    })
                    .transpose()?
                    .unwrap_or(2.0),
            })
        })
        .transpose()?;
    let preview_offset_cm = placement
        .and_then(|data| data.attribute_named_any(&["icon3Doffset"]))
        .map(parse_f32_triplet)
        .transpose()?
        .unwrap_or_default()
        .map(|metres| (metres * 100.0).round() as i16);
    output.document.objects.push(WorldObjectDefinition {
        selected_ui_broadcasts: super::object_selected_ui_broadcast_source_lowering::lower_object_selected_ui_broadcasts(record),
        detach_actions: super::object_detach_action_source_lowering::lower_object_detach_actions(record),
        supports_show_tricks: !super::source_element_tree_search::authored_type_family_components(
            record,
            "ZTAITrickComponent",
        )
        .is_empty(),
        view_data: Vec::new(),
        information_panel:
            super::entity_information_panel_source_lowering::selected_entity_information_panel(
                record,
            )?,
        id: id(record.key),
        kind: world_object_kind(required(record, &["kind", "objectType", "category"])?)?,
        view_class: world_object_information_view_class(record)?,
        biome_automatic_placement_class: world_object_biome_automatic_placement_class(record),
        name_key: asset(record, &["nameKey", "displayName", "displayNameToken"]),
        description_key: asset(record, &["descriptionKey", "description", "helpText"]),
        zoopedia_subject: authored_zoopedia_subject_from_world_object_record(record)?,
        prefab: asset(record, &["prefab", "model", "visual"]),
        catalogue_preview_prefab: None,
        presentation_attachments: Vec::new(),
        named_physical_presentations: Vec::new(),
        interaction_slots: super::interaction_container_source_lowering::lower_authored_world_object_interaction_slots(record)?,
        container_quantity:
            super::container_quantity_source_lowering::lower_authored_container_quantity(record)?,
        transactions: super::authored_transaction_lowering::lower_object_transactions(record)?,
        prefab_scale: 1.0,
        icon: asset(record, &["icon", "iconPath", "purchaseIcon"]),
        biomes,
        location: biome_data
            .and_then(|data| data.attribute_named_any(&["location"]))
            .map(id)
            .unwrap_or_default(),
        preview_offset_cm,
        preview_scale: placement
            .and_then(|data| data.attribute_named_any(&["icon3Dscale"]))
            .and_then(parse_blue_fang_source_numeric_lexeme)
            .unwrap_or(1.0),
        terrain_fitted: bool_or(record, &["terrainFitted", "groundFit"], false)?,
        real_physics_water_impact,
        price_cents: money_cents_i64_or(record, &["priceCents"], &["purchasePrice", "cost"])?,
        upkeep_cents_per_month: number_or(
            record,
            &["upkeepCentsPerMonth", "upkeep", "monthlyCost"],
            0,
        )?,
        properties: WorldObjectPropertyFlags::from_raw_flag_bits(flags(
            record.value(&["tags", "objectFlags"]),
            object_tag,
        )?)
        .ok_or_else(|| BindError::record(record, "object tags contain unknown flag bits"))?,
        affordances: WorldObjectAffordanceFlags::from_raw_flag_bits(flags(
            record.value(&["affordances", "uses", "actions"]),
            affordance,
        )?)
        .ok_or_else(|| BindError::record(record, "object affordances contain unknown flag bits"))?,
        destruction,
    });
    Ok(())
}

pub(super) fn world_object_biome_automatic_placement_class(
    record: &RecordView<'_, '_>,
) -> Option<WorldObjectBiomeAutomaticPlacementClass> {
    if record.has_type_token("tree") {
        Some(WorldObjectBiomeAutomaticPlacementClass::Tree)
    } else if record.has_type_token("plant") || record.has_type_token("marineplant") {
        Some(WorldObjectBiomeAutomaticPlacementClass::Plant)
    } else if record.has_type_token("rock") {
        Some(WorldObjectBiomeAutomaticPlacementClass::Rock)
    } else {
        None
    }
}

pub(super) fn bind_placeable(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let footprint = record
        .children_named(&["cell", "footprintCell"])
        .into_iter()
        .map(|cell| {
            Ok(FootprintCell {
                offset: [
                    element_number(&cell, &["x"], 0)?,
                    element_number(&cell, &["y", "z"], 0)?,
                ],
                flags: FootprintCellFlags::from_raw_flag_bits(flags(
                    cell.attribute_named_any(&["flags"]),
                    footprint_flag,
                )? as u16)
                .ok_or_else(|| {
                    BindError::record(record, "footprint cell contains unknown flag bits")
                })?,
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    let entrances = record
        .children_named(&["entrance", "entry", "accessPoint"])
        .into_iter()
        .map(|entrance| {
            Ok(EntranceDefinition {
                position_cm: element_array(&entrance, &["x", "y", "z"], [0_i16; 3])?,
                forward_snorm: element_array(
                    &entrance,
                    &["forwardX", "forwardY", "forwardZ"],
                    [0_i16, 0, 32767],
                )?,
                purpose: entrance_purpose(
                    entrance
                        .attribute_named_any(&["purpose", "type"])
                        .unwrap_or("guest"),
                )?,
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    output.document.placeables.push(PlaceableDefinition {
        id: id(record.key),
        weight: number_or(record, &["weight"], 1.0)?,
        footprint: footprint.clone(),
        pivot_cm: array(record, &["pivotX", "pivotY"], [0_i16; 2])?,
        diagonal_footprint: footprint,
        diagonal_pivot_cm: array(record, &["pivotX", "pivotY"], [0_i16; 2])?,
        rotation_increment_degrees: number_or(record, &["rotationIncrement", "rotationSnap"], 90)?,
        constraints: PlacementConstraints::from_raw_flag_bits(flags(
            record.value(&["constraints", "placementFlags"]),
            placement_flag,
        )? as u32)
        .ok_or_else(|| {
            BindError::record(record, "object placement contains unknown constraint bits")
        })?,
        max_slope_permille: number_or(record, &["maxSlopePermille", "maxSlope"], 0)?,
        minimum_headroom_metres: f32::from(number_or::<u16>(
            record,
            &["minClearanceCm", "clearance"],
            0,
        )?) * 0.01,
        apply_height_modifier: true,
        price_cents: money_cents_i64_or(record, &["priceCents"], &["cost"])?,
        unlock: asset(record, &["unlock", "unlockRequirement"]),
        entrances,
        moving_footprint: moving_footprint(record)?,
        automatic_footprint: false,
        ground_paths_block_placement: false,
    });
    Ok(())
}

pub(super) fn moving_footprint(record: &RecordView<'_, '_>) -> Result<bool, BindError> {
    let names = &[
        "b_movingEntityWithGridFootprint",
        "movingEntityWithGridFootprint",
    ];
    let value = record.value(names).or_else(|| {
        record
            .descendant_named("BFAIEntityDataShared")
            .and_then(|shared| shared.attribute_named_any(names))
    });
    value
        .map(
            |value| match canonicalize_source_document_record_key(value).as_str() {
                "true" | "yes" | "1" => Ok(true),
                "false" | "no" | "0" => Ok(false),
                _ => Err(BindError::record(
                    record,
                    format!("invalid boolean {value}"),
                )),
            },
        )
        .transpose()
        .map(|value| value.unwrap_or(false))
}
