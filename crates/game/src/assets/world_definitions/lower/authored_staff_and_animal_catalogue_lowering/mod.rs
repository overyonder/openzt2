use super::authored_zoopedia_subject_from_world_object_record;
use super::catalogue_entry_source_lowering::{
    authored_catalogue_purchase_sort_fallback_type_name, authored_catalogue_purchase_sort_key,
};
use super::source_element_tree_search::{
    authored_type_family_component_attribute, find_descendant, find_descendant_with_attribute,
    find_first_authored_family_variant_with_nonempty_attribute, find_presentation_component,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    element_bool, id, money_cents, parse, parse_f32_triplet,
};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::{
    CatalogueCategory, CatalogueEntry, CatalogueFilterFlags,
};
use openzt2_game_data::world_definitions::staff_management::{
    StaffJobCapabilityFlags, StaffRoleDefinition, StaffRoleKind,
};
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectAffordanceFlags, WorldObjectDefinition, WorldObjectKind, WorldObjectPropertyFlags,
};
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::blue_fang_actor_manifest_model_and_scene_resolution_index::BlueFangActorManifestModelAndSceneResolutionIndex;
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::path::AssetPath;
    use crate::assets::world_definitions::world_definition_source_document_lowering::lower_resolved_world_definition_source_document_closure_to_canonical_document;

    #[test]
    fn staff_portrait_uses_authored_head_scene_without_changing_world_body(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sources = [
            (
                "staff.xml",
                r#"<BFTypedBinder binderType="Staff" abstract="true">
                <types><entity><actor><people><Staff/></people></actor></entity></types>
                <shared><ZTPlacementData iconObj="headObj" icon3Doffset="0 0.002026 0.487109" icon3Dscale="3.954455"/></shared>
                <binder><BFBinder><instance><BFGCollisionTester radius="0.1"/></instance></BFBinder></binder>
            </BFTypedBinder>"#,
            ),
            (
                "worker_f.xml",
                r#"<BFTypedBinder binderType="Worker_Adult_F" abstract="true">
                <types><entity><actor><people><Staff><Worker><Worker_Adult_F/></Worker></Staff></people></actor></entity></types>
                <binder><BFNamedBinder binderName="mainObj"><instance><BFPhysObj><BFActorComponent actorfile="body.bfm"/></BFPhysObj></instance></BFNamedBinder>
                <BFNamedBinder binderName="headObj"><instance><BFPhysObj><BFSimpleLODComponent modelfile="head.nif"/></BFPhysObj></instance></BFNamedBinder></binder>
            </BFTypedBinder>"#,
            ),
            (
                "worker.xml",
                r#"<BFTypedBinder binderType="Worker" abstract="true">
                <types><entity><actor><people><Staff><Worker/></Staff></people></actor></entity></types>
                <shared><UIToggleButton><UIAspect><default image="worker.dds"/></UIAspect><on><event msg="ZT_SETPLACEMENTOBJECT" string="Worker"/></on></UIToggleButton></shared>
            </BFTypedBinder>"#,
            ),
        ];
        for separate_head_variant in [false, true] {
            let mut fixture = sources
                .iter()
                .map(|(path, xml)| (*path, (*xml).to_owned()))
                .collect::<Vec<_>>();
            if separate_head_variant {
                let head = r#"<BFNamedBinder binderName="headObj"><instance><BFPhysObj><BFSimpleLODComponent modelfile="head.nif"/></BFPhysObj></instance></BFNamedBinder>"#;
                fixture[1].1 = fixture[1].1.replace(head, "");
                fixture.push(("worker_f_01.xml", format!(r#"<BFTypedBinder binderType="Worker_Adult_F_01">
                <types><entity><actor><people><Staff><Worker><Worker_Adult_F><Worker_Adult_F_01/></Worker_Adult_F></Worker></Staff></people></actor></entity></types>
                <binder>{head}</binder></BFTypedBinder>"#)));
            }
            let documents = fixture
                .into_iter()
                .map(|(path, xml)| {
                    parse_blue_fang_source_document(AssetPath::new(path), xml.as_bytes())
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
            let mut scenes = BlueFangActorManifestModelAndSceneResolutionIndex::default();
            scenes.register_archive_resolved_actor_scene_asset_path(
                "body.bfm",
                "body.nif#scene".into(),
            );
            scenes.register_archive_resolved_actor_scene_asset_path(
                "head.nif",
                "head.nif#scene".into(),
            );
            let document =
                lower_resolved_world_definition_source_document_closure_to_canonical_document(
                    &documents,
                    &scenes,
                    "worker.xml",
                    [0, 0],
                )?
                .ok_or_else(|| std::io::Error::other("staff source produced no document"))?;
            let object = document
                .objects
                .first()
                .ok_or_else(|| std::io::Error::other("staff object is absent"))?;
            assert_eq!(object.prefab, AssetId::from_virtual_path("body.nif#scene"));
            assert_eq!(
                object.catalogue_preview_prefab,
                Some(AssetId::from_virtual_path("head.nif#scene"))
            );
            assert_eq!(object.preview_offset_cm, [0, 0, 49]);
            assert!((object.preview_scale - 3.954_455).abs() < 0.000_001);
        }
        Ok(())
    }
}

fn authored_staff_primary_actor_component<'document>(
    record: &RecordView<'_, 'document>,
) -> Option<&'document OrderedSourceDocumentNode> {
    find_descendant_with_attribute(
        record.source_document_element(),
        "BFNamedBinder",
        "binderName",
        "mainObj",
    )
    .and_then(|main_object| find_descendant(main_object, "BFActorComponent"))
    .filter(|component| {
        component
            .attribute_named_any(&["actorfile"])
            .is_some_and(|value| !value.trim().is_empty())
    })
}

fn authored_staff_named_presentation_component<'document>(
    record: &RecordView<'_, 'document>,
    binder_name: &str,
) -> Option<&'document OrderedSourceDocumentNode> {
    find_descendant_with_attribute(
        record.source_document_element(),
        "BFNamedBinder",
        "binderName",
        binder_name,
    )
    .and_then(find_presentation_component)
}

fn authored_initial_staff_animation(
    record: &RecordView<'_, '_>,
) -> Result<Option<(String, bool)>, BindError> {
    let Some(locomotion_switch_set) = record.descendant_named("BFBehLocoSwitchSet") else {
        return Ok(None);
    };
    let Some(ground_behavior) = find_descendant(locomotion_switch_set, "behaviorTable")
        .and_then(|behavior_table| find_descendant(behavior_table, "ground"))
    else {
        return Err(BindError::record(
            record,
            "authored staff locomotion switch has no ground behavior",
        ));
    };
    let animation_clip_asset_key = ground_behavior
        .attribute_named_any(&["behSet"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| BindError::record(record, "authored staff ground behavior has no behSet"))?
        .to_owned();
    let animation_loops = element_bool(&locomotion_switch_set, &["loopFlag"], false)?;
    Ok(Some((animation_clip_asset_key, animation_loops)))
}

fn find_first_authored_staff_family_variant_with_primary_actor<'index, 'document>(
    record: &RecordView<'index, 'document>,
    resolved_source_records: &[RecordView<'index, 'document>],
) -> Option<RecordView<'index, 'document>> {
    resolved_source_records
        .iter()
        .copied()
        .filter(|candidate| {
            candidate.key != record.key
                && candidate.has_type_token(record.key)
                && authored_staff_primary_actor_component(candidate).is_some()
        })
        .min_by(|left, right| left.key.cmp(right.key))
}

pub(super) fn bind_authored_staff_catalogue(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
    actor_scene_paths: &BTreeMap<String, String>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let Some(purchase_button) = record.descendant_named("UIToggleButton").filter(|_| {
        record
            .descendant_with_attribute("event", "msg", "ZT_SETPLACEMENTOBJECT")
            .is_some()
    }) else {
        return Ok(());
    };
    let Some(actor_record) = authored_staff_primary_actor_component(record)
        .map(|_| *record)
        .or_else(|| {
            find_first_authored_staff_family_variant_with_primary_actor(
                record,
                resolved_source_records,
            )
        })
    else {
        return Ok(());
    };
    let actor = authored_staff_primary_actor_component(&actor_record)
        .and_then(|component| component.attribute_named_any(&["actorfile"]))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            BindError::record(
                record,
                "authored staff variant has no BFActorComponent actorfile",
            )
        })?;
    let actor_key = canonicalize_source_document_record_key(actor);
    let prefab = actor_scene_paths.get(&actor_key).ok_or_else(|| {
        BindError::record(
            record,
            format!("authored staff actor {actor:?} has no resolved model scene"),
        )
    })?;
    let icon = find_descendant(purchase_button, "default")
        .and_then(|element| element.attribute_named_any(&["image"]))
        .map(id)
        .unwrap_or_default();
    let name_key = find_descendant(purchase_button, "UIHelpInfo")
        .and_then(|element| element.attribute_named_any(&["ids"]))
        .map(id)
        .unwrap_or_default();
    let economy = actor_record
        .descendant_named("ZTEconomyComponent")
        .or_else(|| record.descendant_named("ZTEconomyComponent"));
    let price_cents = economy
        .and_then(|component| component.attribute_named_any(&["cost"]))
        .map(|cost| {
            let value = parse_blue_fang_source_numeric_lexeme::<f64>(cost)
                .ok_or_else(|| BindError::record(record, "invalid authored staff hire cost"))?;
            money_cents(value, record).map(i64::from)
        })
        .transpose()?
        .unwrap_or_default();
    let salary = find_descendant_with_attribute(
        actor_record.source_document_element(),
        "ZTTransaction",
        "name",
        "salary",
    )
    .or_else(|| {
        find_descendant_with_attribute(
            record.source_document_element(),
            "ZTTransaction",
            "name",
            "salary",
        )
    });
    let wage_cents_per_month = salary
        .and_then(|transaction| transaction.attribute_named_any(&["cost"]))
        .map(|cost| {
            let value = parse_blue_fang_source_numeric_lexeme::<f64>(cost).ok_or_else(|| {
                BindError::record(record, "invalid authored monthly staff salary")
            })?;
            money_cents(value, record)
        })
        .transpose()?
        .unwrap_or_default();
    if salary
        .and_then(|transaction| transaction.attribute_named_any(&["period"]))
        .is_some_and(|period| !source_document_names_are_semantically_equal(period, "monthly"))
    {
        return Err(BindError::record(
            record,
            "authored staff salary does not use the supported monthly cadence",
        ));
    }
    let key = canonicalize_source_document_record_key(record.key);
    let (role, permitted_jobs) = if key == "keeper" {
        (
            StaffRoleKind::Keeper,
            StaffJobCapabilityFlags::FEED
                .with_additional_capabilities(StaffJobCapabilityFlags::REFILL_WATER)
                .with_additional_capabilities(StaffJobCapabilityFlags::CLEAN_HABITAT)
                .with_additional_capabilities(StaffJobCapabilityFlags::TREAT),
        )
    } else if key == "worker" {
        (
            StaffRoleKind::Maintenance,
            StaffJobCapabilityFlags::EMPTY_BIN
                .with_additional_capabilities(StaffJobCapabilityFlags::SWEEP_LITTER)
                .with_additional_capabilities(StaffJobCapabilityFlags::REPAIR)
                .with_additional_capabilities(StaffJobCapabilityFlags::MAINTAIN_TANK),
        )
    } else if key == "educator" {
        (StaffRoleKind::Educator, StaffJobCapabilityFlags::EDUCATE)
    } else if key == "trainer" {
        (
            StaffRoleKind::Trainer,
            StaffJobCapabilityFlags::OPERATE_SHOW,
        )
    } else if key == "mc" {
        (
            StaffRoleKind::Presenter,
            StaffJobCapabilityFlags::ENTERTAIN
                .with_additional_capabilities(StaffJobCapabilityFlags::OPERATE_SHOW),
        )
    } else if key.starts_with("entertainer") {
        (
            StaffRoleKind::Entertainer,
            StaffJobCapabilityFlags::ENTERTAIN,
        )
    } else if key == "paleontologist" {
        (
            StaffRoleKind::Paleontologist,
            StaffJobCapabilityFlags::CAPTURE,
        )
    } else if key.starts_with("dinorecovery") {
        (
            StaffRoleKind::Recovery,
            StaffJobCapabilityFlags::TRANQUILIZE
                .with_additional_capabilities(StaffJobCapabilityFlags::CAPTURE),
        )
    } else {
        return Err(BindError::record(
            record,
            format!("authored hireable staff role {key:?} has no canonical StaffRoleKind"),
        ));
    };
    let move_speed_mps = actor_record
        .descendant_named("slow")
        .and_then(|slow| slow.attribute_named_any(&["minAnimSpeed"]))
        .map(|value| parse(value, record, "staff minimum movement speed"))
        .transpose()?
        .unwrap_or(1.4);
    let navigation_radius_m =
        authored_type_family_component_attribute(record, "BFGCollisionTester", "radius")
            .map(|value| parse(value, record, "staff navigation radius"))
            .transpose()?
            .ok_or_else(|| {
                BindError::record(
                    record,
                    "authored staff family has no BFGCollisionTester radius",
                )
            })?;
    let mut type_tokens = record.type_tokens();
    if type_tokens.is_empty() {
        type_tokens.push(record.key.to_owned());
    }
    let kind = type_tokens
        .last()
        .map(|value| id(value))
        .unwrap_or_else(|| id(record.key));
    let kinds = type_tokens.iter().map(|value| id(value)).collect();
    let definition = id(record.key);
    let (initial_animation_clip_asset_key, initial_animation_loops) =
        authored_initial_staff_animation(record)?
            .map_or((None, false), |(asset_key, loops)| (Some(asset_key), loops));

    let preview_offset_cm =
        authored_type_family_component_attribute(record, "ZTPlacementData", "icon3Doffset")
            .map(parse_f32_triplet)
            .transpose()?
            .unwrap_or_default()
            .map(|metres| (metres * 100.0).round() as i16);
    let preview_scale =
        authored_type_family_component_attribute(record, "ZTPlacementData", "icon3Dscale")
            .map(|value| parse::<f32>(value, record, "staff catalogue preview scale"))
            .transpose()?
            .unwrap_or(1.0);
    if !preview_scale.is_finite() || preview_scale <= 0.0 {
        return Err(BindError::record(
            record,
            "staff catalogue preview scale must be positive and finite",
        ));
    }
    let catalogue_preview_prefab =
        authored_type_family_component_attribute(record, "ZTPlacementData", "iconObj")
            .filter(|binder| !source_document_names_are_semantically_equal(binder, "mainObj"))
            .map(|binder_name| {
                let component =
                    authored_staff_named_presentation_component(&actor_record, binder_name)
                        .or_else(|| {
                            resolved_source_records
                                .iter()
                                .filter(|candidate| candidate.has_type_token(actor_record.key))
                                .filter_map(|candidate| {
                                    authored_staff_named_presentation_component(
                                        candidate,
                                        binder_name,
                                    )
                                    .map(|component| (candidate.key, component))
                                })
                                .min_by(|left, right| left.0.cmp(right.0))
                                .map(|(_, component)| component)
                        })
                        .ok_or_else(|| {
                            BindError::record(
                                record,
                                format!("staff iconObj {binder_name:?} has no presentation"),
                            )
                        })?;
                let model = component
                    .attribute_named_any(&["modelfile", "actorfile"])
                    .expect("presentation lookup requires a model reference");
                actor_scene_paths
                    .get(&canonicalize_source_document_record_key(model))
                    .map(|scene| AssetId::from_virtual_path(scene))
                    .ok_or_else(|| {
                        BindError::record(
                            record,
                            format!("staff iconObj model {model:?} has no resolved scene"),
                        )
                    })
            })
            .transpose()?;

    output.document.objects.push(WorldObjectDefinition {
        selected_ui_broadcasts: super::object_selected_ui_broadcast_source_lowering::lower_object_selected_ui_broadcasts(record),
        detach_actions: super::object_detach_action_source_lowering::lower_object_detach_actions(record),
        supports_show_tricks: !super::source_element_tree_search::authored_type_family_components(
            record,
            "ZTAITrickComponent",
        )
        .is_empty(),
        view_data: Vec::new(),
        id: definition,
        kind: WorldObjectKind::Staff,
        information_panel:
            super::entity_information_panel_source_lowering::selected_entity_information_panel(
                record,
            )?,
        view_class: None,
        biome_automatic_placement_class: None,
        name_key,
        description_key: AssetId::default(),
        zoopedia_subject: authored_zoopedia_subject_from_world_object_record(record)?,
        prefab: AssetId::from_virtual_path(prefab),
        catalogue_preview_prefab,
        presentation_attachments: Vec::new(),
        named_physical_presentations: Vec::new(),
        interaction_slots: super::interaction_container_source_lowering::lower_authored_world_object_interaction_slots(record)?,
        container_quantity: None,
        transactions: super::authored_transaction_lowering::lower_object_transactions(record)?,
        prefab_scale: 1.0,
        icon,
        biomes: Vec::new(),
        location: AssetId::default(),
        preview_offset_cm,
        preview_scale,
        terrain_fitted: true,
        real_physics_water_impact: None,
        price_cents,
        upkeep_cents_per_month: 0,
        properties: WorldObjectPropertyFlags::default(),
        affordances: WorldObjectAffordanceFlags::default(),
        destruction: None,
    });
    output.document.staff.push(StaffRoleDefinition {
        id: definition,
        object: definition,
        // Hireable source binders describe the role, while shipped person-name
        // data is keyed by the concrete adult presentation. The catalogue
        // currently lowers one deterministic presentation per role, so resolve
        // its matching name category here rather than retaining a reference to
        // the nonexistent generic role pool.
        name_pool: id(&format!("personname:{}_Adult_M", record.key)),
        role,
        model_animation_set: AssetId::from_virtual_path(&actor_key),
        initial_animation_clip_asset_key,
        initial_animation_loops,
        wage_cents_per_month,
        move_speed_mps,
        navigation_radius_m,
        permitted_jobs,
        job_overrides: Vec::new(),
    });
    output.document.catalogue.push(CatalogueEntry {
        filter_values: super::catalogue_entry_source_lowering::authored_catalogue_filter_values(
            record,
        ),
        id: id(&format!("catalogue/{}", record.key)),
        definition,
        kind,
        kinds,
        category: CatalogueCategory::Staff,
        authored_purchase_sort_key: authored_catalogue_purchase_sort_key(record),
        authored_purchase_sort_fallback_type_name:
            authored_catalogue_purchase_sort_fallback_type_name(record),
        authored_type_registry_source_order: [u64::MAX; 3],
        filters: CatalogueFilterFlags::HIREABLE,
        name_key,
        icon,
    });
    Ok(())
}

/// Lowers the authored species-level adoption offer without treating its
/// abstract binder as a live source object.
///
/// Original animal records put the shared purchase icon, name, price, actor
/// presentation, and placement target on the abstract species binder. The
/// concrete sex/life-stage binders are variants and must not each become a
/// catalogue row. Lowering folds that source inheritance convention into one
/// canonical species/world-definition identity here.
pub(super) fn bind_authored_animal_catalogue(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
    actor_scene_paths: &BTreeMap<String, String>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if !record.has_type_token("animal") {
        return Ok(());
    }
    let Some(purchase_button) = record.descendant_named("UIToggleButton").filter(|_| {
        record
            .descendant_with_attribute("event", "msg", "ZT_SETPLACEMENTOBJECT")
            .is_some()
    }) else {
        return Ok(());
    };
    let Some(actor_record) = record
        .descendant_named("BFActorComponent")
        .and_then(|component| component.attribute_named_any(&["actorfile"]))
        .filter(|value| !value.trim().is_empty())
        .map(|_| *record)
        .or_else(|| {
            find_first_authored_family_variant_with_nonempty_attribute(
                record,
                resolved_source_records,
                "BFActorComponent",
                "actorfile",
            )
        })
    else {
        return Ok(());
    };
    let actor = actor_record
        .descendant_named("BFActorComponent")
        .and_then(|component| component.attribute_named_any(&["actorfile"]))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            BindError::record(
                record,
                "authored animal adoption binder has no BFActorComponent actorfile",
            )
        })?;
    let prefab_scale = actor_record
        .descendant_named("BFActorComponent")
        .and_then(|component| component.attribute_named_any(&["scale"]))
        .map(|value| {
            parse_blue_fang_source_numeric_lexeme::<f32>(value)
                .filter(|scale| scale.is_finite() && *scale > 0.0)
                .ok_or_else(|| {
                    BindError::record(
                        record,
                        "authored animal presentation scale must be finite and positive",
                    )
                })
        })
        .transpose()?
        .unwrap_or(1.0);
    let icon = find_descendant(purchase_button, "default")
        .and_then(|element| element.attribute_named_any(&["image"]))
        .map(id)
        .unwrap_or_default();
    let name_key = find_descendant(purchase_button, "UIHelpInfo")
        .and_then(|element| element.attribute_named_any(&["ids"]))
        .map(id)
        .unwrap_or_default();
    let price_cents = record
        .descendant_named("ZTEconomyComponent")
        .and_then(|element| element.attribute_named_any(&["cost"]))
        .map(|cost| {
            let value = parse_blue_fang_source_numeric_lexeme::<f64>(cost)
                .ok_or_else(|| BindError::record(record, "invalid authored animal cost"))?;
            money_cents(value, record).map(i64::from)
        })
        .transpose()?
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
    let biome_data = record.descendant_named("BFGBiomeData");
    let mut biomes = biome_data
        .into_iter()
        .flat_map(|data| data.element_children())
        .map(|biome| {
            let primary = match biome
                .attribute_named_any(&["primary"])
                .map(canonicalize_source_document_record_key)
                .as_deref()
            {
                None | Some("false" | "no" | "0") => false,
                Some("true" | "yes" | "1") => true,
                Some(value) => {
                    return Err(BindError::record(
                        record,
                        format!("invalid authored primary biome boolean {value}"),
                    ));
                }
            };
            Ok((!primary, id(biome.name.as_str())))
        })
        .collect::<Result<Vec<_>, _>>()?;
    biomes.sort_by_key(|(not_primary, _)| *not_primary);
    let biomes = biomes
        .into_iter()
        .map(|(_, biome)| biome)
        .collect::<Vec<_>>();
    let location = biome_data
        .and_then(|data| data.attribute_named_any(&["location"]))
        .map(id)
        .unwrap_or_default();
    let mut type_tokens = record.type_tokens();
    if type_tokens.is_empty() {
        type_tokens.push(record.key.to_owned());
    }
    let kind = type_tokens
        .last()
        .map(|value| id(value))
        .unwrap_or_else(|| id(record.key));
    let kinds = type_tokens.iter().map(|value| id(value)).collect();
    let definition = id(record.key);

    let actor_key = canonicalize_source_document_record_key(actor);
    let prefab = actor_scene_paths.get(&actor_key).ok_or_else(|| {
        BindError::record(
            record,
            format!("authored animal actor {actor:?} has no resolved model scene"),
        )
    })?;
    output.document.objects.push(WorldObjectDefinition {
        selected_ui_broadcasts: super::object_selected_ui_broadcast_source_lowering::lower_object_selected_ui_broadcasts(record),
        detach_actions: super::object_detach_action_source_lowering::lower_object_detach_actions(record),
        supports_show_tricks: !super::source_element_tree_search::authored_type_family_components(
            record,
            "ZTAITrickComponent",
        )
        .is_empty(),
        view_data: Vec::new(),
        id: definition,
        kind: WorldObjectKind::Animal,
        information_panel:
            super::entity_information_panel_source_lowering::selected_entity_information_panel(
                record,
            )?,
        view_class: None,
        biome_automatic_placement_class: None,
        name_key,
        description_key: AssetId::default(),
        zoopedia_subject: authored_zoopedia_subject_from_world_object_record(record)?,
        prefab: AssetId::from_virtual_path(prefab),
        presentation_attachments: Vec::new(),
        named_physical_presentations: Vec::new(),
        catalogue_preview_prefab: None,
        interaction_slots: super::interaction_container_source_lowering::lower_authored_world_object_interaction_slots(record)?,
        container_quantity: None,
        transactions: super::authored_transaction_lowering::lower_object_transactions(record)?,
        prefab_scale,
        icon,
        biomes,
        location,
        preview_offset_cm,
        preview_scale,
        terrain_fitted: true,
        real_physics_water_impact: None,
        price_cents,
        upkeep_cents_per_month: 0,
        properties: WorldObjectPropertyFlags::default(),
        affordances: WorldObjectAffordanceFlags::default(),
        destruction: None,
    });
    output.document.catalogue.push(CatalogueEntry {
        id: id(&format!("catalogue/{}", record.key)),
        definition,
        kind,
        kinds,
        category: CatalogueCategory::Animals,
        filter_values: super::catalogue_entry_source_lowering::authored_catalogue_filter_values(
            record,
        ),
        authored_purchase_sort_key: authored_catalogue_purchase_sort_key(record),
        authored_purchase_sort_fallback_type_name:
            authored_catalogue_purchase_sort_fallback_type_name(record),
        authored_type_registry_source_order: [u64::MAX; 3],
        filters: CatalogueFilterFlags::ADOPTABLE,
        name_key,
        icon,
    });
    Ok(())
}
