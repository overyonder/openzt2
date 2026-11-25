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
use crate::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::object_placement::{
    EntranceDefinition, FootprintCell, FootprintCellFlags, PlaceableDefinition,
    PlacementConstraints,
};
use openzt2_game_data::world_definitions::world_objects::{
    AuthoredWorldObjectRuntime, WorldObjectAffordanceFlags,
    WorldObjectBiomeAutomaticPlacementClass, WorldObjectDefinition,
    WorldObjectDestructionDefinition, WorldObjectEggDefinition, WorldObjectPropertyFlags,
    WorldObjectRealPhysicsWaterImpactDefinition, WorldObjectVirtualNode,
};
use openzt2_game_data::AssetId;

#[cfg(test)]
mod tests;

/// `ZTEntityDestructionData` plus the inherited `f_BuildingStrength`.
pub(super) fn lower_world_object_destruction(
    record: &RecordView<'_, '_>,
) -> Result<Option<WorldObjectDestructionDefinition>, BindError> {
    record
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
                effect_path: format!("particle/{}.psys", effect.trim().to_ascii_lowercase()),
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
                // Buildings inherit it from an abstract type binder (`bathroomsmall`).
                strength: crate::species::source::source_queries::find_effective_authored_animal_shared_attribute(
                    record,
                    "f_BuildingStrength",
                )
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .map(|value| value.round() as u16),
            })
        })
        .transpose()
}

/// Authored `virtualNodes`. A node's own attributes override an ancestor's
/// one by one, as a food stand moves `building`'s rampage docks closer but
/// keeps their yaw.
pub(crate) fn lower_world_object_virtual_nodes(
    record: &RecordView<'_, '_>,
) -> Vec<WorldObjectVirtualNode> {
    const KEYS: [&str; 4] = ["x", "y", "z", "yaw"];
    let mut nodes: Vec<(String, [Option<f32>; 4])> = Vec::new();
    for group in super::source_element_tree_search::authored_type_family_elements_named(
        record,
        "virtualNodes",
    ) {
        for node in group.element_children() {
            let name = node.name.as_str();
            let index = match nodes
                .iter()
                .position(|(known, _)| known.eq_ignore_ascii_case(name))
            {
                Some(index) => index,
                None => {
                    nodes.push((name.to_owned(), [None; 4]));
                    nodes.len() - 1
                }
            };
            for (value, key) in nodes[index].1.iter_mut().zip(KEYS) {
                *value = value.or_else(|| {
                    node.attribute_named_any(&[key])
                        .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
                        .filter(|value| value.is_finite())
                });
            }
        }
    }
    // A position attribute such as a guest's `p_TalkOffset="0 -1 180"`
    // (x, y, yaw) names a node too, where an animal docks to talk or attack.
    for shared in super::source_element_tree_search::authored_type_family_elements_named(
        record,
        "BFAIEntityDataShared",
    ) {
        for attribute in &shared.attributes {
            let name = attribute.name();
            if !name.starts_with("p_")
                || nodes
                    .iter()
                    .any(|(known, _)| known.eq_ignore_ascii_case(name))
            {
                continue;
            }
            let values = attribute
                .value()
                .split_whitespace()
                .map(parse_blue_fang_source_numeric_lexeme::<f32>)
                .collect::<Option<Vec<_>>>();
            if let Some([x, y, yaw]) = values.as_deref() {
                nodes.push((name.to_owned(), [Some(*x), Some(*y), None, Some(*yaw)]));
            }
        }
    }
    nodes
        .into_iter()
        .map(|(name, values)| {
            let [x, y, z, yaw] = values.map(|value| value.unwrap_or(0.0));
            WorldObjectVirtualNode {
                name,
                translation_m: crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates(
                    [x, y, z],
                ),
                // Source yaw turns the other way about the up axis: a dock at
                // `x="-4"` with `yaw="-90"` faces the building, along +X.
                yaw_degrees: -yaw,
            }
        })
        .collect()
}

pub(crate) fn lower_authored_world_object_runtime(
    record: &RecordView<'_, '_>,
) -> Result<AuthoredWorldObjectRuntime, BindError> {
    use super::source_element_tree_search::authored_type_family_elements_named;

    let mut morph_targets = std::collections::BTreeMap::new();
    let mut numeric_behavior_properties = std::collections::BTreeMap::new();
    let shared = authored_type_family_elements_named(record, "BFAIEntityDataShared");
    let clip = |attribute| {
        shared
            .iter()
            .find_map(|group| group.attribute_named_any(&[attribute]))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let fall_animation_clips =
        openzt2_game_data::world_definitions::world_objects::WorldObjectFallAnimationClips {
            airborne: clip("s_FallAnim"),
            land_impact: clip("s_LandImpactAnim"),
            water_impact: clip("s_WaterImpactAnim"),
        };
    let prop_rotation_offset_radians = shared
        .iter()
        .find_map(|group| group.attribute_named_any(&["f_PropRotationOffset"]))
        .map(|degrees| {
            parse_blue_fang_source_numeric_lexeme::<f32>(degrees)
                .filter(|degrees| degrees.is_finite())
                .map(|degrees| -degrees.to_radians())
                .ok_or_else(|| BindError::record(record, "invalid prop rotation offset"))
        })
        .transpose()?
        .unwrap_or_default();
    for portal in authored_type_family_elements_named(record, "ZTTankPortal") {
        for name in ["gateHeight", "waterSurfaceOffset"] {
            if let Some(value) = portal.attribute_named_any(&[name]) {
                let value = parse_blue_fang_source_numeric_lexeme::<f32>(value)
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| BindError::record(record, format!("invalid tank portal {name}")))?;
                numeric_behavior_properties.entry(id(name)).or_insert(value);
            }
        }
    }
    for group in authored_type_family_elements_named(record, "BFAIEntityDataInstance") {
        for attribute in &group.attributes {
            let name = attribute.name();
            if name.eq_ignore_ascii_case("f_ConstructionLevel") || name.eq_ignore_ascii_case("f_Damage") {
                if let Some(value) = parse_blue_fang_source_numeric_lexeme::<f32>(attribute.value()).filter(|value| value.is_finite()) {
                    numeric_behavior_properties.entry(id(name)).or_insert(value);
                }
            }
        }
    }
    for group in shared {
        for attribute in &group.attributes {
            let name = attribute.name();
            if name
                .get(..2)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("s_"))
            {
                morph_targets
                    .entry(id(name))
                    .or_insert_with(|| id(attribute.value()));
            }
            if name.get(..2).is_some_and(|prefix| prefix.eq_ignore_ascii_case("f_")) {
                if let Some(value) = parse_blue_fang_source_numeric_lexeme::<f32>(attribute.value()).filter(|value| value.is_finite()) {
                    numeric_behavior_properties.entry(id(name)).or_insert(value);
                }
            }
        }
    }
    let is_egg = record
        .type_tokens()
        .iter()
        .any(|name| name.eq_ignore_ascii_case("egg"));
    let mut egg = None;
    if is_egg {
        let incubation = authored_type_family_elements_named(record, "BFAIStateVar")
            .into_iter()
            .find(|variable| {
                variable
                    .attribute_named_any(&["Name"])
                    .is_some_and(|name| name.eq_ignore_ascii_case("incubation"))
            });
        if let Some(incubation) = incubation {
            let rate = authored_type_family_elements_named(record, "BFAINeedAdjusts")
                .into_iter()
                .find_map(|adjusts| adjusts.attribute_named_any(&["incubation"]));
            let number = |text: Option<&str>| {
                text.filter(|value| !value.trim().is_empty())
                    .map(|value| {
                        parse_blue_fang_source_numeric_lexeme::<f32>(value)
                            .filter(|value| value.is_finite())
                            .ok_or_else(|| BindError::record(record, "invalid incubation value"))
                    })
                    .transpose()
            };
            let initial_incubation = number(incubation.attribute_named_any(&["Value"]))?
                .or(number(incubation.attribute_named_any(&["ValueMin"]))?)
                .unwrap_or_default();
            let trigger_incubation = number(incubation.attribute_named_any(&["TriggerThreshold"]))?
                .ok_or_else(|| BindError::record(record, "incubation has no trigger threshold"))?;
            egg = Some(WorldObjectEggDefinition {
                initial_incubation,
                incubation_per_update: number(rate)?.unwrap_or_default(),
                trigger_incubation,
            });
        }
    }
    let fossil_assembly_surface =
        !super::source_element_tree_search::authored_type_family_components(
            record,
            "ZTFossilSkeletonComponent",
        )
        .is_empty();
    let whap_physics = lower_whap_physics(record)?;
    Ok(AuthoredWorldObjectRuntime {
        whap_physics,
        fall_animation_clips,
        prop_rotation_offset_radians,
        morph_targets: morph_targets.into_iter().collect(),
        numeric_behavior_properties: numeric_behavior_properties.into_iter().collect(),
        egg,
        fossil_assembly_surface,
    })
}

fn lower_whap_physics(
    record: &RecordView<'_, '_>,
) -> Result<
    Option<openzt2_game_data::world_definitions::world_objects::WorldObjectWhapPhysics>,
    BindError,
> {
    use super::source_element_tree_search::authored_type_family_elements_named;
    use openzt2_game_data::world_definitions::world_objects::WorldObjectWhapPhysics;

    if let Some(physics) = authored_type_family_elements_named(record, "BFRealPhysicsComponent")
        .into_iter()
        .next()
    {
        let number = |name, fallback| {
            physics
                .attribute_named_any(&[name])
                .map(|value| {
                    parse_blue_fang_source_numeric_lexeme::<f32>(value)
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| BindError::record(record, format!("invalid {name}")))
                })
                .transpose()
                .map(|value| value.unwrap_or(fallback))
        };
        if physics
            .attribute_named_any(&["shapeType"])
            .is_some_and(|shape| shape.eq_ignore_ascii_case("box"))
        {
            return Ok(None);
        }
        let speed = [number("minWhackSpeed", 4.0)?, number("maxWhackSpeed", 4.0)?];
        let lob_pitch_degrees = physics
            .attribute_named_any(&["doLobWhack"])
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
            .then(|| {
                Ok::<_, BindError>([number("minLobPitch", 80.0)?, number("maxLobPitch", 85.0)?])
            })
            .transpose()?;
        return Ok(Some(WorldObjectWhapPhysics::Real {
            speed,
            lob_pitch_degrees,
            fit_state: physics
                .attribute_named_any(&["state"])
                .and_then(parse_blue_fang_source_numeric_lexeme)
                .unwrap_or_default(),
        }));
    }
    Ok(
        authored_type_family_elements_named(record, "BFFakePhysicsComponent")
            .into_iter()
            .find_map(|physics| {
                Some(WorldObjectWhapPhysics::Animated {
                    motion_clip: physics
                        .attribute_named_any(&["moveAnim"])?
                        .trim()
                        .to_owned(),
                    rotate_to_heading: !physics
                        .attribute_named_any(&["rotate"])
                        .is_some_and(|value| value.eq_ignore_ascii_case("false")),
                })
            }),
    )
}

pub(super) fn bind_object(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let destruction = lower_world_object_destruction(record)?;
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
    let (presentation_caustic_policies, surface_effect_strength, traversal_animation_manifests) =
        super::authored_presentation_physics_source_lowering::lower_authored_presentation_physics(
            record,
        )?;
    if let Some(maintenance) =
        super::container_quantity_source_lowering::lower_authored_trash_container_maintenance(
            record,
        )?
    {
        output.document.maintenance_definitions.push(maintenance);
    }
    output.document.placement_target_facts.insert(
        id(record.key),
        super::authored_placement_stomp_lowering::lower_authored_placement_target_facts(record),
    );
    output.document.object_record_keys.push((
        super::world_definition_source_value_reading_and_conversion::id(record.key),
        record.key.to_string(),
    ));
    output.document.objects.push(WorldObjectDefinition {
        authored_runtime: lower_authored_world_object_runtime(record)?,
        audio_cue_prefixes: super::authored_audio_cue_prefixes(record),
        behavior_type_identifiers: crate::source_document::authored_behavior_type_identifiers::authored_behavior_type_identifiers(record),
        selected_ui_broadcasts: super::object_selected_ui_broadcast_source_lowering::lower_object_selected_ui_broadcasts(record),
        detach_actions: super::object_detach_action_source_lowering::lower_object_detach_actions(record),
        water_filter: None,
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
        surface_effect_strength,
        presentation_caustic_policies,
        traversal_animation_manifests,
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
        terrain_fit_height_offset_m: 0.0,
        terrain_fit_water_offset_m: None,
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
        virtual_nodes: lower_world_object_virtual_nodes(record),
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
        surface_policy: super::authored_placement_footprint_lowering::authored_surface_policy(
            record,
        )?,
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
        adjacent_to_water: false,
        price_cents: money_cents_i64_or(record, &["priceCents"], &["cost"])?,
        unlock: asset(record, &["unlock", "unlockRequirement"]),
        entrances,
        moving_footprint: moving_footprint(record)?,
        automatic_footprint: false,
        stomp: Default::default(),
        dock_arrow: None,
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
