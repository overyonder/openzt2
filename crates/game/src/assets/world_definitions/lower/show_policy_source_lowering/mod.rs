use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    asset, element_asset, element_number, id, number_or, optional_seconds_ns,
    required_element_number, required_number, seconds_ns, source_distance_cm,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::animal_shows_and_training::{
    ShowAudioCue, ShowEditorPresentationPolicy, ShowPresentationIcon, ShowPresentationPolicy,
    ShowSchedulingPolicy, ShowScoreBand, ShowScoringPolicy, ShowSizeBand,
};

pub(super) fn bind_show_policy(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if output.document.show_scheduling.is_some() {
        return Err(BindError::record(record, "duplicate resolved show policy"));
    }
    let general = record.descendant_record("General").unwrap_or(*record);
    for songs in record.children_named(&["ShowSongs"]) {
        output
            .document
            .show_audio_cues
            .extend(songs.element_children().map(|song| {
                ShowAudioCue {
                    id: id(song
                        .attribute_named_any(&["songName", "name"])
                        .unwrap_or(song.name.as_str())),
                    sound: song
                        .attribute_named_any(&["Sound", "sound"])
                        .map(id)
                        .unwrap_or_default(),
                }
            }));
    }
    let mut score_bands = Vec::new();
    for table in record.children_named(&["ScoreTable"]) {
        for row in table
            .element_children()
            .filter(|child| canonicalize_source_document_record_key(child.name.as_str()) == "row")
        {
            let minimum_tricks = required_element_number(&row, &["MinTricks"])?;
            for entry in row.element_children().filter(|child| {
                canonicalize_source_document_record_key(child.name.as_str()) == "entry"
            }) {
                score_bands.push(ShowScoreBand {
                    minimum_tricks,
                    minimum_score: required_element_number(&entry, &["MinScore"])?,
                    show_score: required_element_number(&entry, &["ShowScore"])?,
                });
            }
        }
    }
    let mut size_bands = Vec::new();
    for table in record.children_named(&["ShowSizeTable"]) {
        for band in table.element_children() {
            let minimum = element_number(&band, &["MinGuests", "NumGuests"], 0_u16)?;
            size_bands.push(ShowSizeBand {
                minimum_show_score: required_element_number(&band, &["MinShowScore"])?,
                guest_count: [
                    minimum,
                    element_number(&band, &["MaxGuests", "NumGuests"], minimum)?,
                ],
            });
        }
    }
    output.document.show_presentation = Some(ShowPresentationPolicy {
        feedback: [
            asset(&general, &["showFeedback"]),
            asset(&general, &["ShowCancelledFeedback"]),
        ],
        behavior_tokens: [
            asset(&general, &["SummonAnimalTokenName"]),
            asset(&general, &["ReturnAnimalTokenName"]),
            asset(&general, &["DefaultBehaviorTokenName"]),
            asset(&general, &["FailureTreatTokenName"]),
            asset(&general, &["SuccessTreatTokenName"]),
            asset(&general, &["CriticalSuccessTreatTokenName"]),
        ],
        master_of_ceremonies_spawn_node: asset(&general, &["MCSpawnNodeName"]),
        master_of_ceremonies_type: asset(&general, &["MCTypeName"]),
        master_of_ceremonies_tokens: [
            asset(&general, &["MCPrepareShowTokenName"]),
            asset(&general, &["MCExecuteShowTokenName"]),
            asset(&general, &["MCCleanUpShowTokenName"]),
            asset(&general, &["MCIdleTokenName"]),
            asset(&general, &["MCReplacePropTokenName"]),
        ],
        master_of_ceremonies_fade_ns: optional_seconds_ns(&general, &["MCFadeInTime"])?,
        master_of_ceremonies_replace_timeout_ns: optional_seconds_ns(
            &general,
            &["MCReplacePropTimeout"],
        )?,
        need_adjustment_category: asset(&general, &["NeedAdjustmentCategoryName"]),
    });
    output.document.show_scheduling = Some(ShowSchedulingPolicy {
        time_category_ns: optional_seconds_ns(&general, &["TimeCategoryDuration"])?,
        donation_token_timeout_ns: optional_seconds_ns(&general, &["DonationTokenTimeout"])?,
        summon_dismiss_timeout_ns: optional_seconds_ns(&general, &["SummonDismissTimeout"])?,
        trick_treat_timeout_ns: optional_seconds_ns(&general, &["DoTrickGetTreatTimeout"])?,
        between_show_ns: optional_seconds_ns(&general, &["BetweenShowsTimeBlock"])?,
        pre_show_ns: optional_seconds_ns(&general, &["PreShowTime"])?,
        post_show_ns: optional_seconds_ns(&general, &["PostShowTime"])?,
        maximum_admitting_ns: optional_seconds_ns(&general, &["MaxAdmittingGuestsTime"])?,
        maximum_travel_cm_per_second: source_distance_cm(
            number_or(&general, &["MaxTravelDistPerSecond"], 0_f64)?,
            record,
        )?
        .max(0) as u32,
        timeslots_per_show: number_or(&general, &["TimeslotsPerShow"], 0)?,
        presentation_update_interval_ns: 0,
        editor_update_interval_ns: 0,
    });
    output.document.show_scoring = Some(ShowScoringPolicy {
        minimum_score: number_or(&general, &["MinShowScore"], 0.0)?,
        percent_tricks_to_score: number_or(&general, &["PercentTricksToScore"], 0.0)?,
        score_update_factor: number_or(&general, &["ShowScoreUpdateFactor"], 0.0)?,
        score_bands,
        size_bands,
    });
    Ok(())
}

pub(super) fn bind_show_presentation_interval(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if output.show_presentation_interval_ns.is_some() {
        return Err(BindError::record(
            record,
            "duplicate resolved show presentation interval",
        ));
    }
    let interval = required_number(record, &["update"])?;
    output.show_presentation_interval_ns = Some(seconds_ns(interval, record)?);
    Ok(())
}

pub(super) fn bind_show_editor_presentation(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if output.document.show_editor_presentation.is_some() {
        return Err(BindError::record(
            record,
            "duplicate resolved show editor presentation",
        ));
    }
    let mut icons = Vec::new();
    for icon_group in record.children_named(&["Icons"]) {
        for icon in icon_group.element_children() {
            icons.push(ShowPresentationIcon {
                id: id(icon.name.as_str()),
                texture: element_asset(&icon, &["icon"]),
            });
        }
    }
    let highlight = record
        .children_named(&["HighlightColor"])
        .first()
        .copied()
        .ok_or_else(|| {
            BindError::record(record, "show editor presentation is missing HighlightColor")
        })?;
    let interval = required_number(record, &["update"])?;
    output.show_editor_interval_ns = Some(seconds_ns(interval, record)?);
    output.document.show_editor_presentation = Some(ShowEditorPresentationPolicy {
        show_score_pips_dimension: required_number(record, &["showScorePipsDimension"])?,
        show_score_scaled_width: required_number(record, &["showScoreScaledWidth"])?,
        animal_score_pips_dimension: required_number(record, &["animalScorePipsDimension"])?,
        animal_score_scaled_width: required_number(record, &["animalScoreScaledWidth"])?,
        droplist_z_adjustment: required_number(record, &["droplistZAdjustment"])?,
        maximum_animals: required_number(record, &["maxAnimalsInShow"])?,
        trick_dropdown_x_adjustment: required_number(record, &["trickDropdownXAdjustment"])?,
        highlight_rgb: [
            required_element_number(&highlight, &["r"])?,
            required_element_number(&highlight, &["g"])?,
            required_element_number(&highlight, &["b"])?,
        ],
        icons: icons.iter().map(|icon| icon.id).collect(),
    });
    output.document.show_presentation_icons.extend(icons);
    Ok(())
}
