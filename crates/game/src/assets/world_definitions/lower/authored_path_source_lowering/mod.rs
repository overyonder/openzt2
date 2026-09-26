use super::authored_zoopedia_subject_from_world_object_record;
use super::catalogue_entry_source_lowering::{
    authored_catalogue_purchase_sort_fallback_type_name, authored_catalogue_purchase_sort_key,
};
use super::fence_and_path_source_lowering::path_width_cm;
use super::source_element_tree_search::{
    authored_type_family_component_attribute, find_descendant,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{id, money_cents, number_or};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::{
    CatalogueCategory, CatalogueEntry, CatalogueFilterFlags,
};
use openzt2_game_data::world_definitions::paths_and_tile_surfaces::{
    GuestPathDefinition, GuestPathTileSurfaceProfile,
};
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectAffordanceFlags, WorldObjectDefinition, WorldObjectKind, WorldObjectPropertyFlags,
};
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

/// Lowers ground and elevated paths into the surface,
/// economy, and catalogue records consumed by topology construction.
pub(super) fn bind_authored_path(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
    entity_scene_paths: &BTreeMap<String, String>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if !source_document_names_are_semantically_equal(
        record.source_document_element().name.as_str(),
        "BFTypedBinder",
    ) || !record.has_type_token("path")
        || record.has_type_token("curb")
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
    let elevated_path = record.descendant_named("ZTPathElevated");
    let Some(path) = elevated_path.or_else(|| record.descendant_named("ZTPath")) else {
        return Ok(());
    };
    let object_id = id(record.key);
    let surface_texture = path
        .attribute_named_any(&["texture", "textureName"])
        .map(id)
        .unwrap_or_default();
    let curb = path
        .attribute_named_any(&["curb"])
        .map(id)
        .unwrap_or_default();
    let purchase_button = record.descendant_named("UIToggleButton");
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
                .ok_or_else(|| BindError::record(record, "invalid authored path cost"))?;
            money_cents(value, record).map(i64::from)
        })
        .transpose()?
        .unwrap_or_default();
    let support_column = |attribute| {
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", attribute)
            .filter(|reference| !reference.trim().is_empty())
            .map(|reference| {
                let support = record
                    .find_resolved_source_record_by_reference(reference)
                    .ok_or_else(|| {
                        BindError::record(record, format!("unresolved path support {reference:?}"))
                    })?;
                super::expanding_column_source_lowering::lower_expanding_column_presentation(
                    &support,
                    "TopPiece1",
                    entity_scene_paths,
                )
            })
            .transpose()
    };
    output.document.paths.push(GuestPathDefinition {
        id: object_id,
        object: object_id,
        surface_texture,
        curb,
        width_cm: path_width_cm(record, resolved_source_records)?,
        capacity: number_or(record, &["capacity"], 0)?,
        speed_permille: number_or(record, &["speedPermille", "speedMultiplier"], 1000)?,
        elevated: elevated_path.is_some(),
        support_prefab: AssetId::default(),
        curve_support_prefab: AssetId::default(),
        support_column: support_column("s_supportType")?,
        curve_support_column: support_column("s_curveSupportType")?,
        max_support_grade_permille: 1000,
        support_headroom_cm: 0,
        surface: GuestPathTileSurfaceProfile { height_cm: [0; 5] },
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
        kind: WorldObjectKind::Path,
        view_class: None,
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
    if record
        .descendant_with_attribute("event", "msg", "ZT_SETPLACEMENTPATH")
        .is_some()
        || (elevated_path.is_some()
            && record
                .descendant_with_attribute("event", "msg", "ZT_SETPLACEMENTOBJECT")
                .is_some())
    {
        let mut type_tokens = record.type_tokens();
        if type_tokens.is_empty() {
            type_tokens.push(record.key.to_owned());
        }
        let kind = type_tokens
            .last()
            .map(|value| id(value))
            .unwrap_or(object_id);
        let kinds = type_tokens.iter().map(|value| id(value)).collect();
        output.document.catalogue.push(CatalogueEntry {
            filter_values: super::catalogue_entry_source_lowering::authored_catalogue_filter_values(
                record,
            ),
            id: id(&format!("catalogue/path/{}", record.key)),
            definition: object_id,
            kind,
            kinds,
            category: CatalogueCategory::Paths,
            authored_purchase_sort_key: authored_catalogue_purchase_sort_key(record),
            authored_purchase_sort_fallback_type_name:
                authored_catalogue_purchase_sort_fallback_type_name(record),
            authored_type_registry_source_order: [u64::MAX; 3],
            filters: CatalogueFilterFlags::BUILDABLE,
            name_key,
            icon,
        });
    }
    Ok(())
}
