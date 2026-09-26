use super::authored_placement_footprint_lowering::{
    authored_footprint_dimensions, authored_placement_bounds_from_shadow_collision_or_dimensions,
    bind_authored_placeable,
};
use super::authored_staff_and_animal_catalogue_lowering::{
    bind_authored_animal_catalogue, bind_authored_staff_catalogue,
};
use super::authored_zoopedia_subject_from_world_object_record;
use super::catalogue_entry_source_lowering::bind_authored_placement_catalogue_entry;
use super::commerce_facility_source_lowering::bind_authored_commerce_facility;
use super::interaction_container_source_lowering::lower_authored_world_object_interaction_slots;
use super::object_and_placeable_source_lowering::world_object_biome_automatic_placement_class;
use super::object_facility_staff_and_guest_source_vocabulary::{
    authored_world_object_information_view_class, authored_world_object_kind,
};
use super::source_element_tree_search::{
    authored_type_family_component_attribute, authored_type_family_elements_named, find_descendant,
    find_descendant_named_with_nonempty_attribute, find_descendant_with_attribute,
    find_presentation_component,
};
use super::source_model_scene_path_normalization::normalize_scene_path;
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    id, money_cents, parse_f32_triplet,
};
use super::world_object_presentation_controller_source_lowering::lower_authored_world_object_presentation_controller_states;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectAffordanceFlags, WorldObjectDefinition, WorldObjectPropertyFlags,
    WorldObjectRealPhysicsWaterImpactDefinition,
};
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

/// Uses the mainObj scene for spawning. Purchase controls also add a catalogue entry.
pub(super) fn bind_authored_placement_object(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
    actor_scene_paths: &BTreeMap<String, String>,
    entity_scene_paths: &BTreeMap<String, String>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if !source_document_names_are_semantically_equal(
        record.source_document_element().name.as_str(),
        "BFTypedBinder",
    ) {
        return Ok(());
    }
    if ((record.has_type_token("fence") || record.has_type_token("gate"))
        && (record.descendant_named("UIToggleButton").is_some()
            || output
                .document
                .objects
                .iter()
                .any(|object| object.id == id(record.key))))
        || record.has_type_token("path")
        || record.has_type_token("curb")
    {
        return Ok(());
    }
    let abstract_binder = record
        .source_document_element()
        .attribute_named_any(&["abstract"])
        .is_some_and(|value| {
            matches!(
                canonicalize_source_document_record_key(value).as_str(),
                "true" | "1"
            )
        });
    if abstract_binder && record.has_type_token("animal") {
        return bind_authored_animal_catalogue(
            record,
            resolved_source_records,
            actor_scene_paths,
            output,
        );
    }
    if abstract_binder && record.has_type_token("staff") {
        return bind_authored_staff_catalogue(
            record,
            resolved_source_records,
            actor_scene_paths,
            output,
        );
    }

    if abstract_binder {
        return Ok(());
    }

    let named_binders = authored_type_family_elements_named(record, "BFNamedBinder");
    let primary_binders = named_binders
        .iter()
        .copied()
        .filter(|binder| {
            binder
                .attribute_named_any(&["binderName"])
                .is_some_and(|name| source_document_names_are_semantically_equal(name, "mainObj"))
        })
        .collect::<Vec<_>>();
    let has_primary_binder = !primary_binders.is_empty();
    let has_presentation_reference = |component: &&OrderedSourceDocumentNode| {
        component
            .attribute_named_any(&["modelfile", "actorfile"])
            .is_some_and(|value| !value.trim().is_empty())
    };
    let direct_component = primary_binders
        .iter()
        .find_map(|binder| find_presentation_component(binder))
        .or_else(|| {
            if !named_binders.is_empty() {
                return None;
            }
            record
                .descendant_named("BFSimpleLODComponent")
                .filter(&has_presentation_reference)
        })
        .or_else(|| {
            if !named_binders.is_empty() {
                return None;
            }
            record
                .descendant_named("BFSceneGraphComponent")
                .filter(&has_presentation_reference)
        })
        .or_else(|| {
            if !named_binders.is_empty() {
                return None;
            }
            record
                .descendant_named("BFRSceneGraphComponent")
                .filter(&has_presentation_reference)
        })
        .or_else(|| {
            if !named_binders.is_empty() {
                return None;
            }
            record
                .descendant_named("BFActorComponent")
                .filter(&has_presentation_reference)
        });
    let direct_model = direct_component
        .and_then(|element| element.attribute_named_any(&["modelfile", "actorfile"]))
        .filter(|value| !value.trim().is_empty());
    let prop_record = record
        .value(&["s_prop", "prop"])
        .and_then(|key| record.find_resolved_source_record_by_reference(key));
    let inherited_prop_component = prop_record
        .as_ref()
        .and_then(|prop| prop.descendant_named("BFSimpleLODComponent"))
        .or_else(|| {
            prop_record
                .as_ref()
                .and_then(|prop| prop.descendant_named("BFSceneGraphComponent"))
        });
    let inherited_prop_model = inherited_prop_component
        .and_then(|element| element.attribute_named_any(&["modelfile"]))
        .filter(|value| !value.trim().is_empty());
    let model = direct_model.or(inherited_prop_model);
    if model.is_none() && !record.has_type_token("entity") {
        return Ok(());
    }
    let presentation_scale = direct_component
        .or(inherited_prop_component)
        .and_then(|component| component.attribute_named_any(&["scale"]))
        .map(|value| {
            parse_blue_fang_source_numeric_lexeme::<f32>(value)
                .filter(|scale| scale.is_finite() && *scale > 0.0)
                .ok_or_else(|| {
                    BindError::record(
                        record,
                        "authored presentation scale must be finite and positive",
                    )
                })
        })
        .transpose()?
        .unwrap_or(1.0);
    let resolved_presentation_scene = direct_component
        .or(inherited_prop_component)
        .zip(model)
        .map(|(component, model)| (component, canonicalize_source_document_record_key(model)))
        .map(|(component, presentation)| {
            let resolved_scenes = if source_document_names_are_semantically_equal(
                component.name.as_str(),
                "BFActorComponent",
            ) {
                actor_scene_paths
            } else {
                entity_scene_paths
            };
            if let Some(scene) = resolved_scenes.get(&presentation) {
                return Ok(Some(scene));
            }
            let tolerates_missing_visual = source_document_names_are_semantically_equal(
                component.name.as_str(),
                "BFSimpleLODComponent",
            ) || primary_binders.iter().any(|binder| {
                find_presentation_component(binder)
                    .is_some_and(|selected| std::ptr::eq(selected, component))
                    && find_descendant(binder, "BFPhysObj").is_some()
            });
            if tolerates_missing_visual {
                bevy::log::warn!(
                    definition = record.key,
                    %presentation,
                    "world object has no presentation model; retaining its logical definition"
                );
                return Ok(None);
            }
            Err(BindError::record(
                record,
                format!("authored presentation {presentation:?} has no resolved model scene"),
            ))
        })
        .transpose()?
        .flatten();
    let purchase_button = record.descendant_named("UIToggleButton").filter(|_| {
        record
            .descendant_with_attribute("event", "msg", "ZT_SETPLACEMENTOBJECT")
            .is_some()
    });
    if model.is_some() && purchase_button.is_none() && !has_primary_binder {
        return Ok(());
    }
    let authored_cardinal =
        authored_footprint_dimensions(record.descendant_named("ZTPlacementData"), "cfootprint")?;
    let automatic_footprint = authored_cardinal.is_none()
        && authored_type_family_component_attribute(record, "ZTPlacementData", "autoFootprint")
            .is_some_and(|value| {
                matches!(
                    canonicalize_source_document_record_key(value).as_str(),
                    "true" | "yes" | "1"
                )
            });
    let placement_bounds = purchase_button.and_then(|_| {
        authored_placement_bounds_from_shadow_collision_or_dimensions(record, authored_cardinal)
    });
    let object_kind = authored_world_object_kind(record);
    let view_class = authored_world_object_information_view_class(record, object_kind);
    let information_button = record.descendant_named("UIToggleButton");
    let icon = information_button
        .and_then(|button| find_descendant(button, "default"))
        .and_then(|element| element.attribute_named_any(&["image"]))
        .map(id)
        .unwrap_or_default();
    let name_key = information_button
        .and_then(|button| find_descendant(button, "UIHelpInfo"))
        .and_then(|element| element.attribute_named_any(&["ids"]))
        .map(id)
        .unwrap_or_default();
    let price_cents = find_descendant_named_with_nonempty_attribute(
        record.source_document_element(),
        "ZTEconomyComponent",
        "cost",
    )
    .and_then(|element| element.attribute_named_any(&["cost"]))
    .map(|cost| {
        let value = parse_blue_fang_source_numeric_lexeme::<f64>(cost)
            .ok_or_else(|| BindError::record(record, "invalid authored placement cost"))?;
        money_cents(value, record).map(i64::from)
    })
    .transpose()?
    .unwrap_or_default();
    let upkeep = find_descendant_with_attribute(
        record.source_document_element(),
        "ZTTransaction",
        "name",
        "upkeep",
    );
    if upkeep
        .and_then(|transaction| transaction.attribute_named_any(&["period"]))
        .is_some_and(|period| !source_document_names_are_semantically_equal(period, "monthly"))
    {
        return Err(BindError::record(
            record,
            "authored object upkeep does not use the supported monthly cadence",
        ));
    }
    let upkeep_cents_per_month = upkeep
        .and_then(|transaction| transaction.attribute_named_any(&["cost"]))
        .map(|cost| {
            let value = parse_blue_fang_source_numeric_lexeme::<f64>(cost)
                .ok_or_else(|| BindError::record(record, "invalid authored monthly upkeep"))?;
            money_cents(value, record)
        })
        .transpose()?
        .unwrap_or_default();
    let prefab = if model.is_some() && resolved_presentation_scene.is_none() {
        AssetId::default()
    } else {
        model.map_or_else(AssetId::default, |model| {
            entity_scene_paths
                .get(record.key)
                .or(resolved_presentation_scene)
                .map_or_else(
                    || AssetId::from_virtual_path(&normalize_scene_path(model)),
                    |resolved_scene| AssetId::from_virtual_path(resolved_scene),
                )
        })
    };
    let named_physical_presentations = if model.is_none() {
        super::named_physical_presentation_source_lowering::lower_named_physical_presentations(
            record,
            entity_scene_paths,
        )?
    } else {
        Vec::new()
    };
    let presentation_attachments = lower_authored_world_object_presentation_controller_states(
        record,
        actor_scene_paths,
        entity_scene_paths,
    )?;
    let terrain_fitted = record.descendant_named("BFGroundFitComponent").is_some()
        || record
            .descendant_named("BFMultiGroundFitComponent")
            .is_some()
        || prop_record.as_ref().is_some_and(|prop| {
            prop.descendant_named("BFGroundFitComponent").is_some()
                || prop.descendant_named("BFMultiGroundFitComponent").is_some()
        });
    let real_physics_water_impact = record
        .descendant_named("BFRealPhysicsComponent")
        .or_else(|| {
            prop_record
                .as_ref()
                .and_then(|prop| prop.descendant_named("BFRealPhysicsComponent"))
        })
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
    let biome_data = record.descendant_named("BFGBiomeData");
    let biomes = biome_data
        .into_iter()
        .flat_map(|data| data.element_children())
        .map(|biome| id(biome.name.as_str()))
        .collect();
    let location = biome_data
        .and_then(|data| data.attribute_named_any(&["location"]))
        .map(id)
        .unwrap_or_default();
    let placement_data = record.descendant_named("ZTPlacementData");
    let preview_offset_cm = placement_data
        .and_then(|data| data.attribute_named_any(&["icon3Doffset"]))
        .map(parse_f32_triplet)
        .transpose()?
        .unwrap_or_default()
        .map(|metres| (metres * 100.0).round() as i16);
    let preview_scale = placement_data
        .and_then(|data| data.attribute_named_any(&["icon3Dscale"]))
        .and_then(parse_blue_fang_source_numeric_lexeme)
        .unwrap_or(1.0);
    if purchase_button.is_some() && automatic_footprint {
        bind_authored_placeable(record, None, true, price_cents, output)?;
    } else if let Some(footprint_bounds) = placement_bounds {
        bind_authored_placeable(record, Some(footprint_bounds), false, price_cents, output)?;
    }
    let interaction_slots = lower_authored_world_object_interaction_slots(record)?;
    let transactions = super::authored_transaction_lowering::lower_object_transactions(record)?;
    bind_authored_commerce_facility(
        record,
        abstract_binder,
        &interaction_slots,
        &transactions,
        output,
    )?;
    let inherited_donation_acceptor = authored_type_family_component_attribute(
        record,
        "BFAIEntityDataShared",
        "b_DonationAcceptor",
    )
    .is_some_and(|value| {
        matches!(
            canonicalize_source_document_record_key(value).as_str(),
            "true" | "yes" | "1"
        )
    });
    let object_property_flags = WorldObjectPropertyFlags::from_raw_flag_bits(
        inherited_donation_acceptor
            .then_some(WorldObjectPropertyFlags::DONATION_ACCEPTOR.raw_flag_bits())
            .unwrap_or_default(),
    )
    .expect("the donation-acceptor flag is part of the canonical flag vocabulary");

    output.document.objects.push(WorldObjectDefinition {
        selected_ui_broadcasts: super::object_selected_ui_broadcast_source_lowering::lower_object_selected_ui_broadcasts(record),
        detach_actions: super::object_detach_action_source_lowering::lower_object_detach_actions(
            record,
        ),
        supports_show_tricks: !super::source_element_tree_search::authored_type_family_components(
            record,
            "ZTAITrickComponent",
        )
        .is_empty(),
        view_data: Vec::new(),
        container_quantity:
            super::container_quantity_source_lowering::lower_authored_container_quantity(record)?,
        information_panel:
            super::entity_information_panel_source_lowering::selected_entity_information_panel(
                record,
            )?,
        id: id(record.key),
        kind: object_kind,
        view_class,
        biome_automatic_placement_class: world_object_biome_automatic_placement_class(record),
        name_key,
        description_key: AssetId::default(),
        zoopedia_subject: authored_zoopedia_subject_from_world_object_record(record)?,
        prefab,
        presentation_attachments,
        named_physical_presentations,
        catalogue_preview_prefab: None,
        interaction_slots,
        transactions,
        prefab_scale: presentation_scale,
        icon,
        biomes,
        location,
        preview_offset_cm,
        preview_scale,
        terrain_fitted,
        real_physics_water_impact,
        price_cents,
        upkeep_cents_per_month,
        properties: object_property_flags,
        affordances: WorldObjectAffordanceFlags::default(),
        destruction: None,
    });
    if purchase_button.is_some() {
        bind_authored_placement_catalogue_entry(record, object_kind, name_key, icon, output);
    }
    Ok(())
}
