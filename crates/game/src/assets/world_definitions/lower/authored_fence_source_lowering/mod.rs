use super::authored_zoopedia_subject_from_world_object_record;
use super::catalogue_entry_source_lowering::{
    authored_catalogue_purchase_sort_fallback_type_name, authored_catalogue_purchase_sort_key,
};
use super::source_element_tree_search::{
    authored_type_family_components, authored_type_family_elements_named, find_descendant,
    find_descendant_with_attribute,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{id, money_cents, number_or};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
#[cfg(test)]
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::{
    CatalogueCategory, CatalogueEntry, CatalogueFilterFlags,
};
use openzt2_game_data::world_definitions::fences_and_gates::{
    FenceDefinition, FenceGatePolicy, FenceSegmentPrefabs, FenceTraversalBlockingFlags,
};
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectAffordanceFlags, WorldObjectDefinition, WorldObjectInformationViewClass,
    WorldObjectKind, WorldObjectPropertyFlags,
};
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

fn authored_fence_traversal_blocking_flags(
    record: &RecordView<'_, '_>,
) -> Result<FenceTraversalBlockingFlags, BindError> {
    let collision_components = authored_type_family_components(record, "BFGCollisionData");
    let parse_score = |value| {
        parse_blue_fang_source_numeric_lexeme::<f64>(value)
            .filter(|score| score.is_finite())
            .ok_or_else(|| BindError::record(record, "invalid authored fence collision score"))
    };
    // BFGCollisionData defaults to 10000. Resolve individual fields through
    // the family: a concrete width-only override must retain the fence score.
    let default_score = collision_components
        .iter()
        .find_map(|component| component.attribute_named_any(&["score"]))
        .map(parse_score)
        .transpose()?
        .unwrap_or(10000.0);
    let mut flag_bits = 0;
    for (source_type, flag) in [
        ("guest", FenceTraversalBlockingFlags::GUEST),
        ("staff", FenceTraversalBlockingFlags::STAFF),
        ("animal", FenceTraversalBlockingFlags::ANIMAL),
        ("vehicle", FenceTraversalBlockingFlags::VEHICLE),
    ] {
        let score = collision_components
            .iter()
            .find_map(|component| {
                find_descendant(*component, "typeScores")
                    .and_then(|scores| scores.attribute_named_any(&[source_type]))
            })
            .map(parse_score)
            .transpose()?
            .unwrap_or(default_score);
        if score > 0.0 {
            flag_bits |= flag.raw_flag_bits();
        }
    }
    FenceTraversalBlockingFlags::from_raw_flag_bits(flag_bits)
        .ok_or_else(|| BindError::record(record, "invalid authored fence traversal flags"))
}

fn authored_fence_purchase_button<'index, 'document>(
    record: &RecordView<'index, 'document>,
) -> Option<&'document OrderedSourceDocumentNode> {
    authored_type_family_elements_named(record, "UIToggleButton")
        .into_iter()
        .find(|button| {
            find_descendant_with_attribute(*button, "event", "msg", "ZT_SETPLACEMENTFENCE")
                .and_then(|event| event.attribute_named_any(&["string"]))
                .is_some_and(|placement_fence| {
                    source_document_names_are_semantically_equal(placement_fence, record.key)
                })
        })
}

/// Lowers each concrete original fence binder into the final topology
/// definition. Named binder children are presentation evidence only; the
/// source inheritance envelope does not survive lowering.
pub(super) fn bind_authored_fence(
    record: &RecordView<'_, '_>,
    entity_scene_paths: &BTreeMap<String, String>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if !source_document_names_are_semantically_equal(
        record.source_document_element().name.as_str(),
        "BFTypedBinder",
    ) || !record.has_type_token("fence")
        || record
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

    let prefab = |name| {
        find_descendant_with_attribute(
            record.source_document_element(),
            "BFNamedBinder",
            "binderName",
            name,
        )
        .and_then(|binder| {
            find_descendant(binder, "BFSimpleLODComponent")
                .or_else(|| find_descendant(binder, "BFSceneGraphComponent"))
                .or_else(|| find_descendant(binder, "BFRSceneGraphComponent"))
                .or_else(|| find_descendant(binder, "BFActorComponent"))
        })
        .and_then(|component| component.attribute_named_any(&["modelfile", "actorfile"]))
        .filter(|path| !path.trim().is_empty())
        .and_then(|path| entity_scene_paths.get(&canonicalize_source_document_record_key(path)))
        .map(|path| AssetId::from_virtual_path(path))
        .unwrap_or_default()
    };
    let cardinal_straight = prefab("fence90");
    if cardinal_straight == AssetId::default() {
        if let Some(button) = find_descendant(record.source_document_element(), "UIToggleButton") {
            if let Some(target) =
                find_descendant_with_attribute(button, "event", "msg", "ZT_SETPLACEMENTFENCE")
                    .and_then(|event| event.attribute_named_any(&["string"]))
                    .filter(|target| {
                        !target.trim().is_empty()
                            && !source_document_names_are_semantically_equal(target, record.key)
                    })
            {
                bind_authored_fence_catalogue_entry(record, button, id(target), output);
            }
        }
        return Ok(());
    }
    let fence = record.descendant_named("ZTFence");
    let gate = fence
        .and_then(|fence| fence.attribute_named_any(&["gate"]))
        .filter(|gate| !gate.trim().is_empty())
        .map(|gate| {
            id(record
                .find_semantically_equivalent_resolved_source_record_key(gate)
                .unwrap_or(gate))
        })
        .unwrap_or_default();
    let gate_prefab = prefab("gate_animation");
    let object_id = id(record.key);
    let is_gate = record.has_type_token("gate");
    let purchase_button = authored_fence_purchase_button(record);
    let icon = purchase_button
        .and_then(|button| find_descendant(button, "default"))
        .and_then(|element| element.attribute_named_any(&["image"]))
        .map(id)
        .unwrap_or_default();
    let name_key = purchase_button
        .and_then(|button| find_descendant(button, "UIHelpInfo"))
        .and_then(|element| element.attribute_named_any(&["ids"]))
        .map(id)
        .unwrap_or_default();
    let price_cents = record
        .descendant_named("ZTEconomyComponent")
        .and_then(|element| element.attribute_named_any(&["cost"]))
        .map(|cost| {
            let value = parse_blue_fang_source_numeric_lexeme::<f64>(cost)
                .ok_or_else(|| BindError::record(record, "invalid authored fence cost"))?;
            money_cents(value, record).map(i64::from)
        })
        .transpose()?
        .unwrap_or_default();
    output.document.fences.push(FenceDefinition {
        id: object_id,
        object: object_id,
        segment_length_cm: 300,
        height_cm: 0,
        strength: number_or(record, &["f_FenceStrength", "fenceStrength"], 0)?,
        blocks: authored_fence_traversal_blocking_flags(record)?,
        gate,
        post_prefab: prefab("start_post"),
        segments: FenceSegmentPrefabs {
            cardinal_straight,
            diagonal_straight: prefab("fence45"),
            cardinal_curve_90: prefab("fence90curve90"),
            diagonal_curve_90: prefab("fence45curve90"),
            cardinal_curve_135: prefab("fence90curve135"),
            diagonal_curve_135: prefab("fence45curve135"),
        },
        gate_policy: FenceGatePolicy {
            prefab: gate_prefab,
            open_animation: AssetId::default(),
            close_animation: AssetId::default(),
            trigger_distance_cm: 0,
            auto_close_ticks: 0,
        },
    });
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
        information_panel:
            super::entity_information_panel_source_lowering::selected_entity_information_panel(
                record,
            )?,
        id: object_id,
        kind: if is_gate {
            WorldObjectKind::Gate
        } else {
            WorldObjectKind::Fence
        },
        view_class: Some(WorldObjectInformationViewClass::Fence),
        biome_automatic_placement_class: None,
        name_key,
        description_key: AssetId::default(),
        zoopedia_subject: authored_zoopedia_subject_from_world_object_record(record)?,
        prefab: AssetId::default(),
        presentation_attachments: Vec::new(),
        named_physical_presentations: Vec::new(),
        catalogue_preview_prefab: None,
        interaction_slots: Vec::new(),
        container_quantity: None,
        transactions: super::authored_transaction_lowering::lower_object_transactions(record)?,
        prefab_scale: 1.0,
        icon,
        biomes: Vec::new(),
        location: AssetId::default(),
        preview_offset_cm: [0; 3],
        preview_scale: 1.0,
        terrain_fitted: false,
        real_physics_water_impact: None,
        price_cents,
        upkeep_cents_per_month: 0,
        properties: WorldObjectPropertyFlags::default(),
        affordances: WorldObjectAffordanceFlags::default(),
        destruction: None,
    });
    if let Some(button) = purchase_button {
        bind_authored_fence_catalogue_entry(record, button, object_id, output);
    }
    Ok(())
}

fn bind_authored_fence_catalogue_entry(
    record: &RecordView<'_, '_>,
    button: &OrderedSourceDocumentNode,
    definition: AssetId,
    output: &mut WorldDefinitionLoweringTables,
) {
    let mut type_tokens = record.type_tokens();
    if type_tokens.is_empty() {
        type_tokens.push(record.key.to_owned());
    }
    let kind = type_tokens
        .last()
        .map_or_else(|| id(record.key), |value| id(value));
    output.document.catalogue.push(CatalogueEntry {
        filter_values: super::catalogue_entry_source_lowering::authored_catalogue_filter_values(
            record,
        ),
        id: id(&format!("catalogue/fence/{}", record.key)),
        definition,
        kind,
        kinds: type_tokens.iter().map(|value| id(value)).collect(),
        category: CatalogueCategory::Fences,
        authored_purchase_sort_key: authored_catalogue_purchase_sort_key(record),
        authored_purchase_sort_fallback_type_name:
            authored_catalogue_purchase_sort_fallback_type_name(record),
        authored_type_registry_source_order: [u64::MAX; 3],
        filters: CatalogueFilterFlags::BUILDABLE,
        name_key: find_descendant(button, "UIHelpInfo")
            .and_then(|element| element.attribute_named_any(&["ids"]))
            .map(id)
            .unwrap_or_default(),
        icon: find_descendant(button, "default")
            .and_then(|element| element.attribute_named_any(&["image"]))
            .map(id)
            .unwrap_or_default(),
    });
}

#[cfg(test)]
mod tests {
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocument;
    use crate::assets::source_document::path::AssetPath;

    use super::*;

    fn parse(path: &str, source: &str) -> OrderedSourceDocument {
        parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes())
            .expect("valid test document")
    }

    #[test]
    fn authored_fence_collision_scores_inherit_and_preserve_broken_and_vehicle_exceptions() {
        let documents = [
            parse(
                "fence.xml",
                r#"<BFTypedBinder binderType="fence" abstract="true">
                    <types><entity><fence/></entity></types>
                    <shared><BFGCollisionData score="100000"><typeScores vehicle="0"/></BFGCollisionData></shared>
                </BFTypedBinder>"#,
            ),
            parse(
                "chainlink.xml",
                r#"<BFTypedBinder binderType="chainlink">
                    <types><entity><fence><chainlink/></fence></entity></types>
                    <shared><BFGCollisionData width="0.25"/></shared>
                </BFTypedBinder>"#,
            ),
            parse(
                "chainlink_broken.xml",
                r#"<BFTypedBinder binderType="chainlink_broken">
                    <types><entity><fence><chainlink><chainlink_broken/></chainlink></fence></entity></types>
                    <shared><BFGCollisionData score="0"/></shared>
                </BFTypedBinder>"#,
            ),
        ];
        let index = SourceIndex::build(&documents).expect("valid source index");
        let intact = authored_fence_traversal_blocking_flags(
            &index.find("chainlink").expect("intact fence"),
        )
        .expect("valid collision scores");
        for flag in [
            FenceTraversalBlockingFlags::ANIMAL,
            FenceTraversalBlockingFlags::GUEST,
            FenceTraversalBlockingFlags::STAFF,
        ] {
            assert!(intact.contains_all(flag));
        }
        assert!(!intact.contains_all(FenceTraversalBlockingFlags::VEHICLE));
        let broken = authored_fence_traversal_blocking_flags(
            &index.find("chainlink_broken").expect("broken fence"),
        )
        .expect("valid broken collision scores");
        assert_eq!(broken.raw_flag_bits(), 0);
    }

    #[test]
    fn concrete_gate_borrows_purchase_button_from_authored_type_family() {
        let documents = [
            parse(
                "entities/objects/fences/ai/gate.xml",
                r#"
                    <BFTypedBinder binderType="gate" abstract="true">
                        <types><entity><fence><gate/></fence></entity></types>
                        <shared>
                            <UIToggleButton template="fence">
                                <UIAspect><default image="staffgate_icon.dds"/></UIAspect>
                                <UIHelpInfo ids="entityname:staffgate"/>
                                <on><event msg="ZT_SETPLACEMENTFENCE" string="staffgate"/></on>
                            </UIToggleButton>
                        </shared>
                    </BFTypedBinder>
                "#,
            ),
            parse(
                "entities/objects/fences/ai/staffgate.xml",
                r#"
                    <BFTypedBinder binderType="staffgate">
                        <types><entity><fence><gate><staffgate/></gate></fence></entity></types>
                        <shared><BFAIEntityDataShared s_uisort="a"/></shared>
                        <binder>
                            <BFNamedBinder binderName="fence90">
                                <instance><BFPhysObj><BFSceneGraphComponent modelfile="staffgate_vert"/></BFPhysObj></instance>
                            </BFNamedBinder>
                        </binder>
                    </BFTypedBinder>
                "#,
            ),
        ];
        let index = SourceIndex::build(&documents).expect("valid source index");
        let staff_gate = index.find("staffgate").expect("indexed staff gate");
        let entity_scene_paths = BTreeMap::from([(
            canonicalize_source_document_record_key("staffgate_vert"),
            "entities/objects/fences/staffgate_vert.nif#Scene".to_owned(),
        )]);
        let mut output = WorldDefinitionLoweringTables::default();

        bind_authored_fence(&staff_gate, &entity_scene_paths, &mut output)
            .expect("staff gate lowers");

        let entry = output.document.catalogue.first().expect("purchase row");
        assert_eq!(entry.definition, id("staffgate"));
        assert_eq!(entry.name_key, id("entityname:staffgate"));
        assert_eq!(entry.icon, id("staffgate_icon.dds"));
        assert_eq!(entry.authored_purchase_sort_key, "a");
    }

    #[test]
    fn concrete_gate_does_not_borrow_another_fence_purchase_target() {
        let documents = [
            parse(
                "entities/objects/fences/ai/gate.xml",
                r#"
                    <BFTypedBinder binderType="gate" abstract="true">
                        <types><entity><fence><gate/></fence></entity></types>
                        <shared>
                            <UIToggleButton template="fence">
                                <UIAspect><default image="staffgate_icon.dds"/></UIAspect>
                                <on><event msg="ZT_SETPLACEMENTFENCE" string="staffgate"/></on>
                            </UIToggleButton>
                        </shared>
                    </BFTypedBinder>
                "#,
            ),
            parse(
                "entities/objects/fences/ai/bamboogate.xml",
                r#"
                    <BFTypedBinder binderType="bamboogate">
                        <types><entity><fence><gate><bamboogate/></gate></fence></entity></types>
                        <binder>
                            <BFNamedBinder binderName="fence90">
                                <instance><BFPhysObj><BFSceneGraphComponent modelfile="bamboogate_vert"/></BFPhysObj></instance>
                            </BFNamedBinder>
                        </binder>
                    </BFTypedBinder>
                "#,
            ),
        ];
        let index = SourceIndex::build(&documents).expect("valid source index");
        let bamboo_gate = index.find("bamboogate").expect("indexed bamboo gate");
        let entity_scene_paths = BTreeMap::from([(
            canonicalize_source_document_record_key("bamboogate_vert"),
            "entities/objects/fences/bamboogate_vert.nif#Scene".to_owned(),
        )]);
        let mut output = WorldDefinitionLoweringTables::default();

        bind_authored_fence(&bamboo_gate, &entity_scene_paths, &mut output)
            .expect("bamboo gate lowers");

        assert!(output.document.catalogue.is_empty());
        let object = output.document.objects.first().expect("topology object");
        assert_eq!(object.name_key, AssetId::default());
        assert_eq!(object.icon, AssetId::default());
    }
}
