use super::object_facility_staff_and_guest_source_vocabulary::{
    guest_memory, guest_need, guest_purpose, guest_rarity, memory_replacement,
};
use super::source_element_tree_search::{
    authored_type_family_component_attribute, authored_type_family_elements_named,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, element_array, element_number, id, money_cents, parse, required,
    required_element, required_number,
};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::guest_simulation_definitions::{
    GuestDefinition, GuestDestinationWeight, GuestMemoryPolicy, GuestNeedDefinition, GuestNeedKind,
    GuestRarity, GuestReactionDefinition, GuestSignedNeedDefinition, MemoryReplacement,
};

pub(super) fn bind_guest(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if source_document_names_are_semantically_equal(
        record.source_document_element().name.as_str(),
        "BFTypedBinder",
    ) {
        return bind_authored_guest_binder(record, output);
    }
    let name_pool = record
        .value(&["namePool"])
        .map(id)
        .unwrap_or_else(|| id(&format!("personname:{}", record.key)));
    let needs = record
        .children_named(&["need", "guestNeed"])
        .into_iter()
        .map(|need| {
            Ok(GuestNeedDefinition {
                kind: guest_need(required_element(&need, &["kind", "name"])?)?,
                initial_permille: element_array(
                    &need,
                    &["minimumInitialPermille", "maximumInitialPermille"],
                    [element_number(&need, &["initialPermille", "initial"], 1000)?; 2],
                )?,
                adjustment_q16: element_number(&need, &["adjustmentQ16"], 0)?,
                reconsider_threshold: element_number(
                    &need,
                    &["reconsiderThreshold", "threshold"],
                    0,
                )?,
                cessation_threshold: need
                    .attribute_named_any(&["cessationThreshold"])
                    .map(|value| parse::<u16>(value, record, "guest need cessation threshold"))
                    .transpose()?,
                critical_threshold: element_number(&need, &["criticalThreshold"], 1_000)?,
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    let destination_weights = record
        .children_named(&["destination", "destinationWeight"])
        .into_iter()
        .map(|destination| {
            Ok(GuestDestinationWeight {
                purpose: guest_purpose(required_element(&destination, &["purpose", "kind"])?)?,
                weight: element_number(&destination, &["weight"], 0)?,
                need: guest_need(
                    destination
                        .attribute_named_any(&["need"])
                        .unwrap_or("social"),
                )?,
                minimum_need: element_number(&destination, &["minimumNeed"], 0)?,
                radius_cm: element_number(&destination, &["radiusCm", "radius"], 0)?,
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    let reactions = record
        .children_named(&["reaction", "memoryReaction"])
        .into_iter()
        .map(|reaction| {
            Ok(GuestReactionDefinition {
                kind: guest_memory(required_element(&reaction, &["kind", "memory"])?)?,
                satisfaction_delta: element_number(&reaction, &["satisfactionDelta"], 0)?,
                education_delta: element_number(&reaction, &["educationDelta"], 0)?,
                retention_ticks: element_number(&reaction, &["retentionTicks"], 0)?,
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    output.document.guests.push(GuestDefinition {
        id: id(record.key),
        object: asset(record, &["object", "entity"]),
        behavior_subject_type_identifiers: {
            let mut authored_type_identifiers = record
                .type_tokens()
                .into_iter()
                .map(|authored_type| id(&authored_type))
                .collect::<Vec<_>>();
            if authored_type_identifiers.is_empty() {
                authored_type_identifiers.push(id(record.key));
            }
            authored_type_identifiers
        },
        name_pool,
        rarity: guest_rarity(required(record, &["rarity", "s_Rarity"])?)?,
        preferred_animal_view_factor_q16: positive_q16(
            required_number(record, &["f_PreferredAnimalViewFactor"])?,
            record,
            "f_PreferredAnimalViewFactor",
        )?,
        starting_cash_cents: array(
            record,
            &["minimumCashCents", "maximumCashCents"],
            [0_i32; 2],
        )?,
        initial_departure_points_q16: required_number(record, &["initialDeparturePointsQ16"])?,
        radius_cm: required_number(record, &["radiusCm", "radius"])?,
        move_speed_mps: required_number(record, &["moveSpeedMps", "moveSpeed"])?,
        patience_ticks: required_number(record, &["patienceTicks"])?,
        needs,
        viewing_need: None,
        happiness_need: None,
        destination_weights,
        reactions,
        memory: GuestMemoryPolicy {
            capacity: required_number(
                record,
                &["memoryCapacity", "viewTargetMemorySize", "tourHistorySize"],
            )?,
            retention_ticks: required_number(record, &["memoryRetentionTicks"])?,
            replacement: memory_replacement(
                record.value(&["memoryReplacement"]).unwrap_or("oldest"),
            )?,
        },
    });
    Ok(())
}

fn bind_authored_guest_binder(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if record
        .source_document_element()
        .attribute_named_any(&["abstract"])
        .is_some_and(|value| {
            matches!(
                canonicalize_source_document_record_key(value).as_str(),
                "true" | "1"
            )
        })
    {
        return Ok(());
    }
    let rarity =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", "s_Rarity")
            .ok_or_else(|| {
                BindError::record(record, "concrete Guest binder has no inherited s_Rarity")
            })?;
    let rarity = match canonicalize_source_document_record_key(rarity).as_str() {
        "common" => GuestRarity::Common,
        "uncommon" => GuestRarity::Uncommon,
        "rare" => GuestRarity::Rare,
        "unique" => return Ok(()),
        value => {
            return Err(BindError::record(
                record,
                format!("concrete Guest binder has unsupported rarity {value:?}"),
            ));
        }
    };
    let name_pool = record
        .type_tokens()
        .into_iter()
        .rev()
        .find(|token| {
            [
                "Guest_Adult_M",
                "Guest_Adult_F",
                "Guest_Young_M",
                "Guest_Young_F",
            ]
            .into_iter()
            .any(|family| source_document_names_are_semantically_equal(token, family))
        })
        .map(|token| id(&format!("personname:{token}")))
        .ok_or_else(|| {
            BindError::record(record, "concrete Guest binder has no person-name family")
        })?;
    let needs = bind_authored_guest_need_definitions(record)?;
    let transaction = authored_type_family_elements_named(record, "ZTTransaction")
        .into_iter()
        .find(|transaction| {
            transaction
                .attribute_named_any(&["name"])
                .is_some_and(|name| {
                    source_document_names_are_semantically_equal(name, "setInitialCash")
                })
        })
        .ok_or_else(|| {
            BindError::record(
                record,
                "concrete Guest binder has no initial-cash transaction",
            )
        })?;
    let cash = ["minCost", "maxCost"].map(|attribute| {
        transaction
            .attribute_named_any(&[attribute])
            .ok_or_else(|| {
                BindError::record(
                    record,
                    format!("initial-cash transaction has no {attribute}"),
                )
            })
            .and_then(|value| parse::<f64>(value, record, attribute))
            .and_then(|value| money_cents(value, record))
    });
    let [minimum_cash, maximum_cash] = cash;
    let starting_cash_cents = [minimum_cash?, maximum_cash?];
    let radius_m = authored_type_family_component_attribute(record, "BFGCollisionTester", "radius")
        .ok_or_else(|| BindError::record(record, "concrete Guest binder has no collision radius"))
        .and_then(|value| parse::<f32>(value, record, "guest collision radius"))?;
    let move_speed_mps = authored_type_family_elements_named(record, "slow")
        .into_iter()
        .find_map(|slow| slow.attribute_named_any(&["minAnimSpeed"]))
        .ok_or_else(|| {
            BindError::record(record, "concrete Guest binder has no slow locomotion speed")
        })
        .and_then(|value| parse::<f32>(value, record, "guest slow locomotion speed"))?;
    let preferred_animal_view_factor_q16 = authored_type_family_component_attribute(
        record,
        "BFAIEntityDataShared",
        "f_PreferredAnimalViewFactor",
    )
    .ok_or_else(|| {
        BindError::record(
            record,
            "concrete Guest binder has no preferred-animal view factor",
        )
    })
    .and_then(|value| parse::<f64>(value, record, "preferred-animal view factor"))
    .and_then(|value| positive_q16(value, record, "f_PreferredAnimalViewFactor"))?;
    let behavior_subject_type_identifiers = record
        .type_tokens()
        .into_iter()
        .map(|authored_type| id(&authored_type))
        .collect();
    let departure_points = authored_type_family_component_attribute(
        record,
        "BFAIEntityDataInstance",
        "f_departurePoints",
    )
    .ok_or_else(|| {
        BindError::record(
            record,
            "concrete Guest binder has no initial departure points",
        )
    })
    .and_then(|value| parse::<f64>(value, record, "f_departurePoints"))?;
    let departure_points_q16 = (departure_points * 65_536.0).round();
    if !departure_points_q16.is_finite()
        || !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&departure_points_q16)
    {
        return Err(BindError::record(
            record,
            "f_departurePoints exceeds stored Q16",
        ));
    }

    output.document.guests.push(GuestDefinition {
        id: id(record.key),
        object: id(record.key),
        behavior_subject_type_identifiers,
        name_pool,
        rarity,
        preferred_animal_view_factor_q16,
        starting_cash_cents,
        initial_departure_points_q16: departure_points_q16 as i32,
        radius_cm: (radius_m * 100.0).round() as u16,
        move_speed_mps,
        patience_ticks: 0,
        needs,
        viewing_need: bind_authored_guest_signed_need(record, "viewanimals")?,
        happiness_need: bind_authored_guest_signed_need(record, "happiness")?,
        destination_weights: Vec::new(),
        reactions: Vec::new(),
        memory: GuestMemoryPolicy {
            capacity: 0,
            retention_ticks: 0,
            replacement: MemoryReplacement::Oldest,
        },
    });
    Ok(())
}

fn bind_authored_guest_signed_need(
    record: &RecordView<'_, '_>,
    attribute: &str,
) -> Result<Option<GuestSignedNeedDefinition>, BindError> {
    let Some(state) = authored_type_family_elements_named(record, "BFAIStateVar")
        .into_iter()
        .find(|state| {
            state
                .attribute_named_any(&["Name"])
                .is_some_and(|name| source_document_names_are_semantically_equal(name, attribute))
        })
    else {
        return Ok(None);
    };
    let q16 = |value: &str, property: &str| {
        let value = (parse::<f64>(value, record, property)? * 65_536.0).round();
        if !value.is_finite() || !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&value) {
            return Err(BindError::record(
                record,
                format!("{property} exceeds signed Q16"),
            ));
        }
        Ok(value as i32)
    };
    let initial = state
        .attribute_named_any(&["Value"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            BindError::record(
                record,
                format!("{attribute} has no supported fixed initial Value"),
            )
        })?;
    let adjustment = authored_type_family_elements_named(record, "BFAINeedAdjusts")
        .into_iter()
        .filter(|adjustment| adjustment.attribute_named_any(&["Name"]).is_none())
        .find_map(|adjustment| adjustment.attribute_named_any(&[attribute]))
        .map(|value| q16(value, attribute))
        .transpose()?
        .unwrap_or_default();
    Ok(Some(GuestSignedNeedDefinition {
        initial_q16: q16(initial, attribute)?,
        adjustment_q16: adjustment,
        trigger_q16: state
            .attribute_named_any(&["TriggerThreshold"])
            .map(|value| q16(value, attribute))
            .transpose()?,
        cessation_q16: state
            .attribute_named_any(&["CessationThreshold"])
            .map(|value| q16(value, attribute))
            .transpose()?,
    }))
}

fn bind_authored_guest_need_definitions(
    record: &RecordView<'_, '_>,
) -> Result<Vec<GuestNeedDefinition>, BindError> {
    let adjustments = authored_type_family_elements_named(record, "BFAINeedAdjusts");
    if !adjustments
        .iter()
        .any(|adjustment| adjustment.attribute_named_any(&["Name"]).is_none())
    {
        return Err(BindError::record(
            record,
            "concrete Guest binder has no default need adjustments",
        ));
    }
    [
        (GuestNeedKind::Hunger, "hunger"),
        (GuestNeedKind::Thirst, "thirst"),
        (GuestNeedKind::Dessert, "dessert"),
        (GuestNeedKind::Gift, "gift"),
        (GuestNeedKind::Energy, "rest"),
        (GuestNeedKind::Restroom, "bathroom"),
        (GuestNeedKind::Social, "social"),
    ]
    .into_iter()
    .map(|(kind, source_name)| {
        let state = authored_type_family_elements_named(record, "BFAIStateVar")
            .into_iter()
            .find(|state| {
                state.attribute_named_any(&["Name"]).is_some_and(|name| {
                    source_document_names_are_semantically_equal(name, source_name)
                })
            })
            .ok_or_else(|| {
                BindError::record(record, format!("Guest need {source_name} has no state"))
            })?;
        let value = state
            .attribute_named_any(&["Value"])
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse::<f64>(value, record, source_name))
            .transpose()?;
        let minimum = value.unwrap_or_else(|| {
            state
                .attribute_named_any(&["ValueMin"])
                .and_then(parse_blue_fang_source_numeric_lexeme)
                .unwrap_or_default()
        });
        let maximum = value.unwrap_or_else(|| {
            state
                .attribute_named_any(&["ValueMax"])
                .and_then(parse_blue_fang_source_numeric_lexeme)
                .unwrap_or(minimum)
        });
        let wellness = |need: f64| (1_000.0 - need * 10.0).clamp(0.0, 1_000.0).round() as u16;
        // Native shared-binder XML merge keeps parent attributes omitted by
        // the child; only an explicitly authored value overrides a parent.
        let adjustment = adjustments
            .iter()
            .filter(|adjustment| adjustment.attribute_named_any(&["Name"]).is_none())
            .find_map(|adjustment| adjustment.attribute_named_any(&[source_name]))
            .map(|value| parse::<f64>(value, record, source_name))
            .transpose()?
            .unwrap_or_default();
        let adjustment_q16 = (-adjustment * 10.0 * 65_536.0)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        let reconsider_threshold = state
            .attribute_named_any(&["TriggerThreshold"])
            .map(|value| parse::<f64>(value, record, "guest need trigger threshold"))
            .transpose()?
            .unwrap_or_default();
        let critical_threshold = state
            .attribute_named_any(&["CriticalThreshold"])
            .map(|value| parse::<f64>(value, record, "guest need critical threshold"))
            .transpose()?
            .unwrap_or(101.0);
        Ok(GuestNeedDefinition {
            kind,
            initial_permille: [wellness(maximum), wellness(minimum)],
            adjustment_q16,
            reconsider_threshold: (reconsider_threshold * 10.0).clamp(0.0, 1_000.0).round() as u16,
            cessation_threshold: state
                .attribute_named_any(&["CessationThreshold"])
                .map(|value| parse::<f64>(value, record, "guest need cessation threshold"))
                .transpose()?
                .map(|value| (value * 10.0).clamp(0.0, 1_000.0).round() as u16),
            critical_threshold: (critical_threshold * 10.0).clamp(0.0, 1_010.0).round() as u16,
        })
    })
    .collect()
}

fn positive_q16(value: f64, record: &RecordView<'_, '_>, property: &str) -> Result<u32, BindError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(BindError::record(
            record,
            format!("{property} must be finite and positive"),
        ));
    }
    let scaled = (value * 65_536.0).round();
    if !(1.0..=f64::from(u32::MAX)).contains(&scaled) {
        return Err(BindError::record(
            record,
            format!("{property} exceeds stored Q16"),
        ));
    }
    Ok(scaled as u32)
}
