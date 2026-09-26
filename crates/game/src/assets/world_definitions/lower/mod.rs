//! Resolves source inheritance and lowers world-definition records.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument as DataDocument,
    OrderedSourceDocumentSpan as DocumentSpan,
};
use crate::assets::source_document::resolved_source_record_index::{
    BindError,
    RecordView,
    SourceIndex,
};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key,
    source_document_names_are_semantically_equal,
};
use crate::assets::world_definitions::behavior_auxiliary_source_types::LoweredBehaviorAuxiliary;
use openzt2_game_data::AssetId;
use openzt2_game_data::world_definitions::aquatic_habitats::TankEditPolicy;
use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueFilterFlags;
use openzt2_game_data::world_definitions::catalogue_and_progression::research_and_unlock_definition_types::{
    UnlockDefinition,
    UnlockRequirement,
};
use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
use openzt2_game_data::world_definitions::facilities_and_maintenance::CleanlinessPolicy;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestViewingPolicy;
use openzt2_game_data::world_definitions::simulation_time::SimulationTimingDefinition;
use self::animal_adoption_offer_configuration_source_lowering::bind_authored_animal_adoption_offer_configuration;
use self::animal_health_source_lowering::{
    bind_disease,
    bind_rampage,
    bind_tranquilizer,
    bind_tranquilizer_mode,
    bind_treatment,
};
use self::aquatic_habitat_source_lowering::{
    bind_aquatic,
    bind_tank,
};
use self::authored_fence_source_lowering::bind_authored_fence;
use self::authored_path_source_lowering::bind_authored_path;
use self::authored_placement_object_source_lowering::bind_authored_placement_object;
use self::biome_definition_and_detail_source_lowering::{
    apply_biome_automatic_placement_supplements,
    bind_biome,
    bind_biome_detail_placement,
};
use self::catalogue_entry_source_lowering::{
    bind_authored_type_list_entry,
    bind_catalogue,
};
use self::environment::{
    bind_ambient,
    bind_environment,
    bind_environment_fog,
    bind_environment_lights,
    bind_location,
    bind_location_entries_document,
    bind_weather,
};
use self::extinct_animal_recovery_source_lowering::{
    bind_cloning_center,
    bind_fossil_piece,
    bind_fossil_puzzle,
    bind_fossil_set,
    bind_fossil_slot,
};
use self::facility_and_maintenance_source_lowering::{
    bind_cleanliness,
    bind_facility,
    bind_maintenance,
};
use self::fence_and_path_source_lowering::{
    bind_fence,
    bind_path,
};
use self::guest_definition_source_lowering::bind_guest;
use self::guest_generation_policy_source_lowering::bind_guest_policy;
use self::guest_viewing_source_lowering::{
    ViewingTemplate,
    bind_guest_viewing_policy,
    bind_viewing_opportunity,
    bind_viewing_template,
};
use self::immersive_mode_and_camera::{
    bind_camera,
    bind_immersive_mode_policy,
};
use self::object_and_placeable_source_lowering::{
    bind_object,
    bind_placeable,
};
use self::person_name_pool::bind_person_names;
use self::progression_definition_source_lowering::{
    bind_award,
    bind_fame,
    bind_rating,
    bind_research,
    bind_unlock,
};
use self::show_and_training_source_lowering::{
    bind_behavior_auxiliary,
    bind_show_rule,
    bind_show_stage,
    bind_trick,
};
use self::show_platform_upgrade_source_lowering::lower_show_platform_upgrade_transactions_to_policy;
use self::show_policy_source_lowering::{
    bind_show_editor_presentation,
    bind_show_policy,
    bind_show_presentation_interval,
};
use self::simulation_timing_source_lowering::bind_timing;
use self::source_element_tree_search::{
    authored_type_family_component_attribute,
    authored_type_family_components,
};
use self::staff_role_and_job_source_lowering::{
    bind_staff,
    bind_staff_job,
};
use self::staff_water_cleaning_policy_source_lowering::bind_staff_water_cleaning;
use self::tank_policy_source_lowering::{
    bind_tank_edit_policy,
    bind_tank_policy,
};
use self::terrain_editing_brush_source_lowering::bind_brush;
use self::tour_policy_source_lowering::{
    lower_authored_tour_category_scores_to_world_definition_tables,
    lower_authored_tour_rating_ranges_to_world_definition_tables,
    lower_authored_tour_scoring_policy_to_world_definition_tables,
};
use self::transportation_and_tour_definition_source_lowering::{
    bind_authored_vehicle_seats,
    bind_station,
    bind_tour_view,
    bind_track,
    bind_vehicle,
};
use self::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use self::world_definition_source_record_classification::{
    WorldDefinitionSourceRecordKind,
    classify_world_definition_source_record,
    is_semantic_world_definition_source_record,
};
use self::world_definition_source_value_reading_and_conversion::{
    id,
    money_cents,
    required_number,
    seconds_ns,
    simple_error,
};
use self::zoopedia_source_lowering::{
    bind_zoopedia,
    bind_zoopedia_entries,
};
use std::collections::BTreeMap;

mod container_quantity_source_lowering;
mod expanding_column_source_lowering;
mod named_physical_presentation_source_lowering;
mod staff_manager_policy_source_lowering;
mod staff_request_controller_source_lowering;

fn authored_zoopedia_subject_from_world_object_record(
    record: &RecordView<'_, '_>,
) -> Result<AssetId, BindError> {
    let authored_hyperlink =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", "s_Zoopedia")
            .or_else(|| record.value(&["s_Zoopedia"]));
    let Some(authored_hyperlink) = authored_hyperlink else {
        return Ok(AssetId::default());
    };
    let mut fields = authored_hyperlink.split(':');
    let namespace = fields.next().unwrap_or_default();
    let subject = fields.next().unwrap_or_default();
    let destination = fields.next().unwrap_or_default();
    if !source_document_names_are_semantically_equal(namespace, "zoopedia")
        || subject.trim().is_empty()
        || !source_document_names_are_semantically_equal(destination, "entry")
        || fields.next().is_some()
    {
        return Err(BindError::record(
            record,
            format!("invalid authored s_Zoopedia hyperlink {authored_hyperlink:?}"),
        ));
    }
    Ok(id(subject))
}

fn bind_authored_profile_lock_to_catalogue_unlock_requirement(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) {
    let Some(profile_lock) =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", "s_ProfileLock")
            .filter(|profile_lock| !profile_lock.trim().is_empty())
    else {
        return;
    };
    let target = id(record.key);
    let mut has_catalogue_entry = false;
    for entry in output
        .document
        .catalogue
        .iter_mut()
        .filter(|entry| entry.definition == target)
    {
        entry.filters = entry
            .filters
            .with_additional_flags(CatalogueFilterFlags::HIDDEN_UNTIL_UNLOCKED);
        has_catalogue_entry = true;
    }
    if has_catalogue_entry {
        output.document.unlocks.push(UnlockDefinition {
            id: id(&format!("profile-lock/{profile_lock}/{}", record.key)),
            target,
            requirement: UnlockRequirement::Profile(id(profile_lock)),
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnmappedSource {
    pub virtual_path: String,
    pub span: DocumentSpan,
    pub root_or_type: String,
    pub reason: String,
}

#[derive(Default)]
struct RecordShard {
    tables: WorldDefinitionLoweringTables,
    unmapped: Option<UnmappedSource>,
    timing: Option<SimulationTimingDefinition>,
    cleanliness: Option<CleanlinessPolicy>,
    admission_price_bands_cents: Option<[i32; 4]>,
    guest_viewing: Option<GuestViewingPolicy>,
    tank_edit_policy: Option<TankEditPolicy>,
}

/// Bind resolved source documents into the canonical per-document domain records.
///
/// The caller must have resolved archive precedence. Source-level `extends`,
/// `base`, `parentType`, `inherit`, and `template` links are followed here.
/// Duplicate resolved IDs, inheritance cycles observed through required facts,
/// invalid typed values, and missing mandatory global policies are errors.
pub(super) fn bind_resolved_world_definition_source_records_to_canonical_document(
    documents: &[DataDocument],
    primary_path: &str,
    auxiliary: Option<&LoweredBehaviorAuxiliary>,
    actor_scene_paths: &BTreeMap<String, String>,
    entity_scene_paths: &BTreeMap<String, String>,
    authored_type_registry_source_order: [u64; 2],
) -> Result<WorldDefinitionDocument, BindError> {
    if auxiliary.is_none() {
        if let Some(document) = documents.iter().find(|document| {
            matches!(
                canonicalize_source_document_record_key(&document.root.name).as_str(),
                "tricks" | "ztpuzzlemgr"
            )
        }) {
            return Err(BindError {
                virtual_path: document.path.key(),
                span: document.root.span,
                message: "trick and fossil-placement documents require auxiliary definitions"
                    .to_owned(),
            });
        }
    }
    let index = SourceIndex::build_with_additional_named_record_element_types(
        documents,
        &["ZTFirstPersonMode", "ZTPhotoMode", "ZTSuperStaffMode"],
    )?;
    let records = index.records().collect::<Vec<_>>();
    let primary_records = records
        .iter()
        .filter(|record| record.source_path() == primary_path)
        .copied()
        .collect::<Vec<_>>();
    let primary_documents = documents
        .iter()
        .filter(|document| document.path.key() == primary_path)
        .cloned()
        .collect::<Vec<_>>();
    let tranquilizer_mode = bind_tranquilizer_mode(&primary_records)?;
    let viewing_template = bind_viewing_template(&records)?;
    let show_platform_upgrades =
        lower_show_platform_upgrade_transactions_to_policy(&primary_documents)?;
    let shards = primary_records
        .iter()
        .map(|record| {
            bind_record_shard(
                record,
                &records,
                viewing_template.as_ref(),
                actor_scene_paths,
                entity_scene_paths,
                authored_type_registry_source_order,
            )
        })
        .collect::<Vec<_>>();
    let mut output = WorldDefinitionLoweringTables::default();
    if primary_path.starts_with("locations/") && primary_path.ends_with("initiallocs.xml") {
        for document in &primary_documents {
            bind_location_entries_document(document, &mut output)?;
        }
    }
    output.document.biome_detail_placement = bind_biome_detail_placement(&primary_documents)?;
    let mut unmapped = Vec::new();
    let mut timing = None;
    let mut cleanliness = None;
    let mut admission_price_bands_cents = None;
    let mut guest_viewing = None;
    let mut tank_edit_policy = None;
    for shard in shards {
        let mut shard = shard?;
        if shard.timing.is_some() {
            timing = shard.timing;
        }
        if shard.cleanliness.is_some() {
            cleanliness = shard.cleanliness;
        }
        if shard.admission_price_bands_cents.is_some() {
            admission_price_bands_cents = shard.admission_price_bands_cents;
        }
        if shard.guest_viewing.is_some() {
            guest_viewing = shard.guest_viewing;
        }
        if shard.tank_edit_policy.is_some() {
            if tank_edit_policy.is_some() {
                return Err(simple_error(
                    "duplicate resolved TerrDeformationUI tank edit policy",
                ));
            }
            tank_edit_policy = shard.tank_edit_policy;
        }
        if let Some(source) = shard.unmapped.take() {
            unmapped.push(source);
        }
        output.append_resolved_world_definition_lowering_shard(shard.tables)?;
    }
    if output.document.show_scheduling.is_some() {
        for record in &records {
            match canonicalize_source_document_record_key(&record.semantic_type()).as_str() {
                "ztshowtimermanager" if output.show_presentation_interval_ns.is_none() => {
                    bind_show_presentation_interval(record, &mut output)?;
                }
                "ztshowmixermanager" if output.show_editor_interval_ns.is_none() => {
                    output.show_editor_interval_ns =
                        Some(seconds_ns(required_number(record, &["update"])?, record)?);
                }
                _ => {}
            }
        }
    }
    apply_biome_automatic_placement_supplements(&documents, &mut output)?;
    if primary_path.starts_with("ui/zoopedia/entries/") {
        bind_zoopedia_entries(documents, &mut output)?;
    } else {
        bind_zoopedia_entries(&primary_documents, &mut output)?;
    }
    if let Some(auxiliary) = auxiliary {
        bind_behavior_auxiliary(auxiliary, &mut output)?;
    }

    output.document.tank_edit_policy = tank_edit_policy;
    if let Some(first) = unmapped.first() {
        let roots = unmapped
            .iter()
            .map(|source| source.root_or_type.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(BindError {
            virtual_path: first.virtual_path.clone(),
            span: first.span,
            message: format!(
                "{} source roots/facts have no typed world-definition binding: {roots}",
                unmapped.len()
            ),
        });
    }
    output.sort_tables_and_reject_duplicate_identifiers()?;
    let mut document = output.finish_canonical_world_definition_document(
        timing,
        cleanliness,
        show_platform_upgrades,
        guest_viewing,
    );
    document.admission_price_bands_cents = admission_price_bands_cents;
    document.tranquilizer_mode = tranquilizer_mode;
    Ok(document)
}

fn bind_record_shard(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
    viewing_template: Option<&ViewingTemplate>,
    actor_scene_paths: &BTreeMap<String, String>,
    entity_scene_paths: &BTreeMap<String, String>,
    authored_type_registry_source_order: [u64; 2],
) -> Result<RecordShard, BindError> {
    let mut shard = RecordShard::default();
    let kind = record.semantic_type();
    if canonicalize_source_document_record_key(&kind) == "ztaimgr" {
        if let Some(show_manager) = record.descendant_record("ZTAIShowMgr") {
            bind_show_policy(&show_manager, &mut shard.tables)?;
        }
    }
    bind_authored_type_list_entry(record, &mut shard.tables)?;
    staff_manager_policy_source_lowering::bind_authored_staff_manager_policy(
        record,
        &mut shard.tables,
    )?;
    staff_request_controller_source_lowering::bind_authored_staff_request_controllers_to_entity_definitions(record, &mut shard.tables)?;
    bind_authored_animal_adoption_offer_configuration(
        record,
        resolved_source_records,
        &mut shard.tables,
    )?;
    bind_authored_fence(record, entity_scene_paths, &mut shard.tables)?;
    bind_authored_path(
        record,
        resolved_source_records,
        entity_scene_paths,
        &mut shard.tables,
    )?;
    bind_authored_placement_object(
        record,
        resolved_source_records,
        actor_scene_paths,
        entity_scene_paths,
        &mut shard.tables,
    )?;
    bind_authored_vehicle_seats(record, &mut shard.tables)?;
    if record
        .descendant_named("ZTCloningCenterComponent")
        .is_some()
    {
        bind_cloning_center(record, &mut shard.tables)?;
    }
    bind_viewing_opportunity(record, viewing_template, &mut shard.tables)?;
    let classified = (canonicalize_source_document_record_key(&kind) == "bfphysobj"
        && (record
            .descendant_named("BFOverheadCameraComponent")
            .is_some()
            || record.descendant_named("BFFPSCameraComponent").is_some()
            || record.descendant_named("BFCameraComponent").is_some()))
    .then_some(WorldDefinitionSourceRecordKind::Camera)
    .or_else(|| {
        (canonicalize_source_document_record_key(&kind) == "zt2strings"
            && record.source_path().ends_with("peoplenames.xml"))
        .then_some(WorldDefinitionSourceRecordKind::PersonNames)
    })
    .or_else(|| {
        (canonicalize_source_document_record_key(&kind) == "terrdeformationui"
            && record.value(&["tankFloorSpeed"]).is_some())
        .then_some(WorldDefinitionSourceRecordKind::TankEdit)
    })
    .or_else(|| {
        (canonicalize_source_document_record_key(&kind) == "entries"
            && record.descendant_named("BFHelpEntry").is_some())
        .then_some(WorldDefinitionSourceRecordKind::Container)
    })
    .or_else(|| {
        (canonicalize_source_document_record_key(&kind) == "entries"
            && record.source_path().starts_with("locations/")
            && record.source_path().ends_with("initiallocs.xml"))
        .then_some(WorldDefinitionSourceRecordKind::Container)
    })
    .or_else(|| {
        (canonicalize_source_document_record_key(&kind) == "terrdeformationui")
            .then_some(WorldDefinitionSourceRecordKind::Container)
    })
    .or_else(|| {
        (source_document_names_are_semantically_equal(
            record.source_document_element().name.as_str(),
            "BFTypedBinder",
        ) && record.has_type_token("guest"))
        .then_some(WorldDefinitionSourceRecordKind::Guest)
    })
    .or_else(|| {
        (source_document_names_are_semantically_equal(
            record.source_document_element().name.as_str(),
            "BFTypedBinder",
        ) && record.has_type_token("station"))
        .then_some(WorldDefinitionSourceRecordKind::Station)
    })
    .or_else(|| {
        (source_document_names_are_semantically_equal(
            record.source_document_element().name.as_str(),
            "BFTypedBinder",
        ) && record.has_type_token("track"))
        .then_some(WorldDefinitionSourceRecordKind::Track)
    })
    .or_else(|| {
        (source_document_names_are_semantically_equal(
            record.source_document_element().name.as_str(),
            "BFTypedBinder",
        ) && record.has_type_token("vehicle"))
        .then_some(WorldDefinitionSourceRecordKind::Vehicle)
    })
    .or_else(|| classify_world_definition_source_record(&kind));
    match classified {
        Some(WorldDefinitionSourceRecordKind::Container)
            if canonicalize_source_document_record_key(&kind) == "ztstatus" =>
        {
            let cheap = money_cents(required_number(record, &["admissionCheap"])?, record)?;
            let moderate = money_cents(required_number(record, &["admissionModerate"])?, record)?;
            let expensive = money_cents(required_number(record, &["admissionExpensive"])?, record)?;
            shard.admission_price_bands_cents = Some([0, cheap, moderate, expensive]);
        }
        Some(WorldDefinitionSourceRecordKind::Container)
            if canonicalize_source_document_record_key(&kind) == "bfgtraversabilitymgr" =>
        {
            shard.guest_viewing = Some(bind_guest_viewing_policy(record)?);
        }
        Some(WorldDefinitionSourceRecordKind::Container) => {}
        Some(WorldDefinitionSourceRecordKind::Object) => bind_object(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Placeable) => {
            bind_placeable(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Facility) => {
            bind_facility(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Maintenance) => {
            bind_maintenance(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Cleanliness) => {
            shard.cleanliness = Some(bind_cleanliness(record)?)
        }
        Some(WorldDefinitionSourceRecordKind::Staff) => bind_staff(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::StaffJob) => {
            bind_staff_job(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Guest) => bind_guest(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Fence) => bind_fence(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Path) => {
            bind_path(record, resolved_source_records, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Biome) => bind_biome(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Brush) => bind_brush(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Disease) => bind_disease(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Treatment) => {
            bind_treatment(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Tranquilizer) => {
            bind_tranquilizer(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Rampage) => bind_rampage(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Catalogue) => {
            bind_catalogue(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Zoopedia) => {
            bind_zoopedia(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Research) => {
            bind_research(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Unlock) => bind_unlock(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Rating) => bind_rating(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Fame) => bind_fame(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Award) => bind_award(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Tank) => bind_tank(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Aquatic) => bind_aquatic(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::ShowStage) => {
            bind_show_stage(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Trick) => bind_trick(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::ShowRule) => {
            bind_show_rule(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Station) => bind_station(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Track) => bind_track(
            record,
            resolved_source_records,
            entity_scene_paths,
            &mut shard.tables,
        )?,
        Some(WorldDefinitionSourceRecordKind::Vehicle) => bind_vehicle(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::TourView) => {
            bind_tour_view(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::FossilSet) => {
            bind_fossil_set(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::FossilPiece) => {
            bind_fossil_piece(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::FossilSlot) => {
            bind_fossil_slot(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::ImmersiveModePolicy) => {
            bind_immersive_mode_policy(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::FossilPuzzle) => {
            bind_fossil_puzzle(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Camera) => bind_camera(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Environment) => {
            bind_environment(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Weather) => bind_weather(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Ambient) => bind_ambient(record, &mut shard.tables)?,
        Some(WorldDefinitionSourceRecordKind::Timing) => {
            shard.timing = Some(bind_timing(record, &mut shard.tables)?)
        }
        Some(WorldDefinitionSourceRecordKind::GuestPolicy) => {
            bind_guest_policy(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::TankPolicy) => {
            bind_tank_policy(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::TankEdit) => {
            shard.tank_edit_policy = Some(bind_tank_edit_policy(record)?)
        }
        Some(WorldDefinitionSourceRecordKind::ShowPolicy) => {
            bind_show_policy(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::ShowTimer) => {
            bind_show_presentation_interval(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::ShowMixer) => {
            bind_show_editor_presentation(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::TourValues) => {
            lower_authored_tour_category_scores_to_world_definition_tables(
                record,
                &mut shard.tables,
            )?
        }
        Some(WorldDefinitionSourceRecordKind::TourPolicy) => {
            lower_authored_tour_scoring_policy_to_world_definition_tables(
                record,
                &mut shard.tables,
            )?
        }
        Some(WorldDefinitionSourceRecordKind::TourRatings) => {
            lower_authored_tour_rating_ranges_to_world_definition_tables(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::StaffWaterCleaning) => {
            bind_staff_water_cleaning(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::EnvironmentFog) => {
            bind_environment_fog(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::EnvironmentLights) => {
            bind_environment_lights(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::PersonNames) => {
            bind_person_names(record, &mut shard.tables)?
        }
        Some(WorldDefinitionSourceRecordKind::Location) => {
            bind_location(record, &mut shard.tables)?
        }
        None if is_semantic_world_definition_source_record(record) => {
            shard.unmapped = Some(UnmappedSource {
                virtual_path: record.source_path(),
                span: record.span(),
                root_or_type: kind,
                reason: "unsupported world-definition source root/type".to_owned(),
            });
        }
        None => {}
    }
    if authored_type_family_components(record, "ZTDevComponent").is_empty() {
        bind_authored_profile_lock_to_catalogue_unlock_requirement(record, &mut shard.tables);
        progression_definition_source_lowering::bind_authored_catalogue_research(
            record,
            &mut shard.tables,
        )?;
    } else {
        shard.tables.document.catalogue.clear();
    }
    for catalogue_entry in &mut shard.tables.document.catalogue {
        catalogue_entry.authored_type_registry_source_order = [
            authored_type_registry_source_order[0],
            authored_type_registry_source_order[1],
            u64::try_from(record.span().start).unwrap_or(u64::MAX),
        ];
    }
    for object in &mut shard.tables.document.objects {
        object.view_data = view_event_data_source_lowering::lower_view_data(record)?;
        object_water_boundary_source_lowering::apply_authored_water_boundary_property(
            record, object,
        )?;
    }
    Ok(shard)
}

mod ambient_class_source_vocabulary;
mod ambient_spawn_element_source_lowering;
mod animal_adoption_offer_configuration_source_lowering;
mod animal_health_source_lowering;
mod aquatic_habitat_source_lowering;
mod authored_fence_source_lowering;
mod authored_path_source_lowering;
mod authored_placement_footprint_lowering;
mod authored_placement_footprint_types;
mod authored_placement_object_source_lowering;
mod authored_staff_and_animal_catalogue_lowering;
mod authored_transaction_lowering;
mod biome_definition_and_detail_source_lowering;
mod catalogue_category_source_vocabulary;
mod catalogue_entry_source_lowering;
mod commerce_facility_source_lowering;
mod entity_information_panel_source_lowering;
mod environment;
mod environment_source_path_identity;
mod extinct_animal_recovery_source_lowering;
mod facility_and_maintenance_source_lowering;
mod fence_and_path_source_lowering;
mod guest_definition_source_lowering;
mod guest_generation_policy_source_lowering;
mod guest_viewing_source_lowering;
mod immersive_mode_and_camera;
mod immersive_mode_policy_source_vocabulary;
mod interaction_container_source_lowering;
mod object_and_placeable_source_lowering;
mod object_detach_action_source_lowering;
mod object_facility_staff_and_guest_source_vocabulary;
mod object_selected_ui_broadcast_source_lowering;
mod object_water_boundary_source_lowering;
mod person_name_pool;
mod progression_definition_source_lowering;
mod show_and_training_source_lowering;
mod show_platform_upgrade_source_lowering;
mod show_policy_source_lowering;
mod simulation_timing_source_lowering;
mod source_element_tree_search;
mod source_model_scene_path_normalization;
mod staff_role_and_job_source_lowering;
mod staff_water_cleaning_policy_source_lowering;
mod tank_policy_source_lowering;
mod terrain_editing_brush_source_lowering;
mod terrain_editing_brush_source_vocabulary;
mod tour_policy_source_lowering;
mod transportation_and_tour_definition_source_lowering;
mod world_definition_lowering_tables;
mod world_definition_source_flag_vocabulary;
mod world_definition_source_record_classification;
mod world_definition_source_value_reading_and_conversion;
mod world_object_presentation_controller_source_lowering;
mod zoo_rating_source_vocabulary;
mod zoopedia_source_lowering;

mod view_event_data_source_lowering;
