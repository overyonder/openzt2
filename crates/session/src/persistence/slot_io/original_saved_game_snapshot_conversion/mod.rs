use super::{
    terrain_snapshot_types::TerrainSnapshotRecord, world_snapshot_types::WorldSnapshotRecord,
};
use bevy::prelude::*;
use openzt2_game_assets::{
    lua_script::{
        lua_script_archive_path_index::LuaScriptArchivePathIndex,
        lua_script_asset_loading::LuaScriptAsset,
    },
    original_saved_game_asset_loading::OriginalSavedGameAsset,
    source_document::{
        ordered_source_document_types::OrderedSourceDocumentNode,
        source_document_semantic_name::canonicalize_source_document_record_key,
    },
    species::species_asset_types::SpeciesView,
    world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView,
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};
use openzt2_game_data::{audio::AudioCrowdPolicy, AssetId};
use openzt2_world_state::simulation_time::{
    deterministic_random_stream::derive_zoo_seed_from_scenario_and_profile,
    simulation_control_types::SimulationControl,
};
use std::io;

#[allow(clippy::too_many_arguments)]
pub(super) fn encode_original_saved_game_snapshot(
    saved: &OriginalSavedGameAsset,
    world: &WorldSnapshotRecord,
    terrain: &TerrainSnapshotRecord,
    profile: AssetId,
    document: &Handle<WorldScenarioDocumentAsset>,
    scripts: &Assets<LuaScriptAsset>,
    script_index: &LuaScriptArchivePathIndex,
    definitions: WorldDefinitionsView<'_>,
    species: SpeciesView<'_>,
    crowd: AudioCrowdPolicy,
) -> io::Result<Vec<u8>> {
    let context = unique_child(&saved.document.root, "BFContext")?;
    let status = unique_child(context, "ZTStatus")?;
    let economy_manager = unique_child(context, "ZTEconomyMgr")?;
    validate_saved_status_progression(status)?;
    validate_inactive_managers(context)?;
    let start = saved
        .source
        .document
        .starting_zoos
        .first()
        .ok_or_else(|| invalid("saved world has no starting record"))?;
    for entity in &start.entities {
        if species.find_variant(entity.definition).is_some() || definitions.find_object(entity.definition).is_some_and(|object|matches!(object.kind,openzt2_game_data::world_definitions::world_objects::WorldObjectKind::Animal|openzt2_game_data::world_definitions::world_objects::WorldObjectKind::Guest|openzt2_game_data::world_definitions::world_objects::WorldObjectKind::Staff)) {
            return Err(invalid(format!("saved actor {} requires its runtime continuation conversion",entity.persistent_id)));
        }
    }
    let timing = definitions.timing();
    let (clock, calendar) =
        super::original_saved_game_clock_conversion::read_original_saved_clock(status, timing)?;
    let native_month = u32::from(
        calendar
            .year
            .checked_sub(timing.calendar.epoch_year)
            .ok_or_else(|| invalid("saved calendar precedes its epoch"))?,
    ) * 12
        + u32::from(calendar.month - 1);
    let economy = super::original_saved_game_economy_conversion::read_original_saved_economy(
        economy_manager,
        status,
        native_month,
        calendar,
        saved.unlimited_cash,
    )?;
    let general =
        super::original_saved_game_general_economy_conversion::read_original_saved_general_economy(
            economy_manager,
            native_month,
            calendar,
        )?;
    let (fame, fame_history_samples) =
        super::original_saved_game_fame_conversion::read_original_saved_fame(status)?;
    let seed =
        derive_zoo_seed_from_scenario_and_profile(world.scenario_definition_identifier, profile);
    let simulation = super::super::simulation_time_snapshot_section::original_saved_game_conversion::from_saved_clock(
        clock,
        calendar,
        SimulationControl::new(0, false),
        seed,
        timing,
    )
    .map_err(|error| invalid(format!("invalid saved simulation state: {error:?}")))?;
    let definition = saved
        .source
        .document
        .scenarios
        .iter()
        .find(|scenario| scenario.id == world.scenario_definition_identifier)
        .ok_or_else(|| invalid("saved world has no exact scenario definition"))?;
    let native_next = start
        .next_persistent_identifier
        .ok_or_else(|| invalid("saved world has no allocator highwater"))?;
    let mut next = start
        .entities
        .iter()
        .map(|row| row.persistent_id)
        .chain(start.paths.iter().map(|row| row.persistent_id))
        .chain(start.topology_nodes.iter().map(|row| row.persistent_id))
        .chain(start.fences.iter().map(|row| row.persistent_id))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| invalid("saved world exhausts canonical identifiers"))?
        .max(native_next);
    let scenario=super::scenario_snapshot_section::original_saved_game_conversion::read_original_saved_scenario_snapshot(
        unique_child(context,"ZTScenarioMgr")?,definition,&saved.source.document.scenario_script_bindings,document.id(),scripts,script_index,clock.tick,
        ||{let id=next;next=next.checked_add(1).ok_or_else(||invalid("saved scenario identifiers are exhausted"))?;Ok(id)},
    )?;
    let favourite =
        super::original_saved_game_favorite_conversion::read_original_saved_favourite_animal_trend(
            unique_child(context, "ZTFavoriteAnimalMgr")?,
            |name| {
                let id = AssetId::from_key(&canonicalize_source_document_record_key(name));
                species.find(id).map(|_| id).ok_or_else(|| {
                    invalid(format!("saved favourite species {name} is unavailable"))
                })
            },
        )?;
    let map = saved
        .source
        .document
        .maps
        .first()
        .ok_or_else(|| invalid("saved world has no map"))?;
    let audio = openzt2_game_assets::audio::source::saved_world::lower_saved_world_audio_preset(
        &saved.document,
        unique_child(context, "ZTWorldSndMgr")?,
        map.biome,
        unique_child(context, "ZTMapData")?
            .attribute("biome")
            .ok_or_else(|| invalid("saved map has no biome name"))?,
        crowd,
    )?;
    let runtime = super::runtime_continuation_snapshot_types::RuntimeContinuationSnapshot {
        general_economy: Some(general),
        entity_names: None,
        world_audio_preset: Some(audio),
        entity_name_counters: Some(
            super::original_saved_game_entity_name_conversion::read_original_saved_entity_name_counters(status)?,
        ),
        registered_entity_name_identifiers: None,
        next_persistent_identifier: next,
        navigation_sequence: 0,
        behavior_execution_sequence: 0,
        service_request_sequence: Some(
            openzt2_zoo_state::economy::service_types::ServiceRequestSequence::default()
                .next_sequence(),
        ),
        guest_arrivals: None,
        guest_survey: Some(
            super::original_saved_game_guest_survey_conversion::read_original_saved_guest_survey(
                unique_child(context, "ZTAIGuestMgr")?,
            )?,
        ),
        guest_favourite_animal_trend: Some(favourite),
    };
    let progression = super::super::progression_snapshot_types::ProgressionSnapshotRecords {
        fame,
        fame_history_samples,
        zoo_rating: Default::default(),
        unlocked_catalogue_definition_count: 0,
        unlocked_catalogue_definition_bit_words: Vec::new(),
        scenario_award_points: 0,
        endangered_animal_birth_count: 0,
        animal_adoption_availability_policy: Default::default(),
        research_projects: Vec::new(),
        earned_progression_awards: Vec::new(),
        award_condition_duration_progress: Vec::new(),
        research_project_availability: Vec::new(),
        species_adoption_chances: Vec::new(),
    };
    let mut bytes = Vec::new();
    super::super::snapshot_container_encoding_and_validation::initialize_empty_world_snapshot_container(&mut bytes,profile);
    super::snapshot_container_section_assembly::append_captured_world_snapshot_sections_and_finalize_container(
        &mut bytes,world,Some(terrain),Some(&scenario),None,&[],&Default::default(),&[],&[],None,&economy,&[],None,&progression,&simulation,&[],&[],&[],&Default::default(),&[],&[],&[],Some(&runtime),
    ).map_err(|error|invalid(format!("cannot encode converted save: {error:?}")))?;
    Ok(bytes)
}

fn validate_inactive_managers(context: &OrderedSourceDocumentNode) -> io::Result<()> {
    for record in context.element_children() {
        if !matches!(
            record.name.as_str(),
            "ZTMapData"
                | "ZTStatus"
                | "ZTEconomyMgr"
                | "ZTWorldMgr"
                | "ZTWorldSndMgr"
                | "BFGElevatedTileSerializer"
                | "BFGInfluenceMgr"
                | "BFGTraversabilityMgr"
                | "BFGManager"
                | "ZTFeedbackMgr"
                | "ZTAIStaffMgr"
                | "ZTAIGuestMgr"
                | "ZTAdoptionMgr"
                | "ZTPhotoChallengesComponent"
                | "ZTFavoriteAnimalMgr"
                | "ZTAwardMgr"
                | "ZTScenarioMgr"
                | "ZTTourData"
                | "ZTTourDataAttr"
                | "ZTTourDataFeedback"
                | "ZTTourRecorderData"
                | "ZTAIAmbientsMgr"
        ) {
            return Err(invalid(format!(
                "saved manager {} has no continuation conversion",
                record.name
            )));
        }
    }
    for name in [
        "BFGElevatedTileSerializer",
        "ZTFeedbackMgr",
        "ZTAIStaffMgr",
        "ZTAdoptionMgr",
        "ZTAwardMgr",
        "ZTTourData",
        "ZTTourDataAttr",
        "ZTTourDataFeedback",
        "ZTTourRecorderData",
        "ZTAIAmbientsMgr",
    ] {
        let manager = unique_child(context, name)?;
        for (attribute, _) in manager.attributes() {
            let understood = matches!(attribute, "version" | "safehandledobjid")
                || (attribute == "cleanupInterval"
                    && matches!(name, "ZTAIStaffMgr" | "ZTAIAmbientsMgr"))
                || (attribute == "uipanel" && name == "ZTAwardMgr");
            if !understood {
                return Err(invalid(format!(
                    "saved {name}.{attribute} requires continuation conversion"
                )));
            }
        }
        if name == "ZTAwardMgr" && manager.attribute("uipanel") != Some("ZTAwardPanel") {
            return Err(invalid("saved award panel differs from ZTAwardPanel"));
        }
        if let Some(expected) = match name {
            "ZTAIStaffMgr" => Some(10.0),
            "ZTAIAmbientsMgr" => Some(5.0),
            _ => None,
        } {
            let interval = manager
                .attribute("cleanupInterval")
                .and_then(|value| value.parse::<f32>().ok());
            if interval != Some(expected) {
                return Err(invalid(format!(
                    "saved {name} cleanupInterval requires policy conversion"
                )));
            }
        }
        if let Some(child) = manager.element_children().next() {
            return Err(invalid(format!(
                "{name}/{} requires runtime continuation conversion",
                child.name
            )));
        }
    }
    let photo = unique_child(context, "ZTPhotoChallengesComponent")?;
    if photo.element_children().next().is_some()
        || photo.attribute("name") != Some("bogusChallengeSetName")
    {
        return Err(invalid(
            "saved photo challenge set requires runtime challenge conversion",
        ));
    }
    let entity_manager = unique_child(context, "BFGManager")?;
    if unique_child(entity_manager, "deleteRecordList")?
        .element_children()
        .next()
        .is_some()
    {
        return Err(invalid(
            "saved entity deletion queue requires continuation conversion",
        ));
    }
    for entity in unique_child(entity_manager, "entityList")?.element_children() {
        for component in entity
            .element_children()
            .filter(|node| node.name == "subComponents")
            .flat_map(|node| node.element_children())
            .filter(|node| node.name == "ZTEconomyComponentStub")
        {
            let id = entity.attribute("globalID").unwrap_or("without globalID");
            if component.attribute("startMonth") != Some("0")
                || component
                    .attribute("startupExpense")
                    .and_then(|value| value.parse::<f32>().ok())
                    != Some(0.0)
            {
                return Err(invalid(format!(
                    "saved entity {id} economy opening-month state requires conversion"
                )));
            }
            let indices = unique_child(component, "transactionCostIndex")?;
            for (transaction, value) in indices.attributes() {
                if value.parse::<u32>().ok() != Some(0) {
                    return Err(invalid(format!("saved entity {id} transaction {transaction} has an unconverted price index {value}")));
                }
            }
        }
    }
    Ok(())
}
fn validate_saved_status_progression(status: &OrderedSourceDocumentNode) -> io::Result<()> {
    for name in ["releasedAnimals", "animalsPutUpForAdoption"] {
        if unique_child(status, name)?
            .element_children()
            .next()
            .is_some()
        {
            return Err(invalid(format!(
                "saved Status.{name} requires historical animal conversion"
            )));
        }
    }
    let half_stars = status
        .attribute("num_halfstars")
        .and_then(|value| value.parse::<f32>().ok())
        .ok_or_else(|| invalid("saved half-star value is invalid"))?;
    let mut fields = std::collections::BTreeSet::new();
    for field in unique_child(status, "script")?.element_children() {
        let name = field
            .attribute("name")
            .ok_or_else(|| invalid("saved Status script field has no name"))?;
        if name.starts_with('_') {
            continue;
        }
        if !fields.insert(name) {
            return Err(invalid(format!("saved Status script repeats {name}")));
        }
        let expected = match name {
            "currentHalfStars" => half_stars,
            "currentNumAnimalFame"
            | "averageGuestHappiness"
            | "currentEducationFame"
            | "currentNumSpeciesFame"
            | "currentEntertainmentFame"
            | "averageAnimalHappiness"
            | "scenarioPoints"
            | "awardPoints" => 0.0,
            _ => {
                return Err(invalid(format!(
                    "saved Status script field {name} has no conversion"
                )))
            }
        };
        let value = field
            .attribute("value")
            .and_then(|value| value.parse::<f32>().ok());
        if field.name != "number" || value != Some(expected) {
            return Err(invalid(format!(
                "saved Status script field {name} requires contribution-state conversion"
            )));
        }
    }
    if fields.len() != 9 {
        return Err(invalid("saved Status script omits its fame/point fields"));
    }
    Ok(())
}

fn unique_child<'a>(
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> io::Result<&'a OrderedSourceDocumentNode> {
    let mut matching = node.element_children().filter(|child| child.name == name);
    let result = matching
        .next()
        .ok_or_else(|| invalid(format!("{} has no {name}", node.name)))?;
    if matching.next().is_some() {
        return Err(invalid(format!("{} repeats {name}", node.name)));
    }
    Ok(result)
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
