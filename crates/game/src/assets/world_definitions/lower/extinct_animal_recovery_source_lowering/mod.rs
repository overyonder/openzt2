use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    asset, asset_list, element_number, id, matrix, number_or, required_element_number,
    required_number, seconds_ns,
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::extinct_animal_recovery::{
    CloningCenterDefinition, CloningDifficultyLevel, CloningFailureOutcome, FossilPieceDefinition,
    FossilSetDefinition, FossilSlotDefinition,
};
use openzt2_game_data::AssetId;

pub(super) fn bind_fossil_set(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let pieces = record
        .value(&["pieces"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.fossil_sets.push(FossilSetDefinition {
        id: id(record.key),
        species: asset(record, &["species"]),
        pieces,
        completion_unlock: asset(record, &["completionUnlock", "unlock"]),
    });
    Ok(())
}

pub(super) fn bind_fossil_piece(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.fossil_pieces.push(FossilPieceDefinition {
        id: id(record.key),
        set: asset(record, &["set", "fossilSet"]),
        model: asset(record, &["model"]),
        slot: asset(record, &["slot"]),
        discovery_weight: required_number(record, &["discoveryWeight", "weight"])?,
        scale_permille: number_or(record, &["scalePermille"], 1000)?,
    });
    Ok(())
}

pub(super) fn bind_fossil_puzzle(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let root = record.source_document_element();
    let animal = root
        .attribute_named_any(&["animal"])
        .ok_or_else(|| BindError {
            virtual_path: record.source_path(),
            span: record.span(),
            message: "ZTPuzzle is missing its animal identity".to_owned(),
        })?;
    let set = AssetId::from_virtual_path(&format!("fossil-sets/{animal}"));
    let mut piece_ids = Vec::new();
    let pieces = root
        .element_children()
        .find(|child| canonicalize_source_document_record_key(child.name.as_str()) == "pieces")
        .into_iter()
        .flat_map(OrderedSourceDocumentNode::element_children);
    for piece in pieces {
        let node = piece
            .attribute_named_any(&["node"])
            .ok_or_else(|| BindError {
                virtual_path: record.source_path(),
                span: piece.span,
                message: "ZTPuzzlePiece is missing its target node".to_owned(),
            })?;
        let texture = piece
            .attribute_named_any(&["texture"])
            .ok_or_else(|| BindError {
                virtual_path: record.source_path(),
                span: piece.span,
                message: "ZTPuzzlePiece is missing its texture".to_owned(),
            })?;
        let scale = piece
            .attribute_named_any(&["entityScale"])
            .unwrap_or("1")
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| BindError {
                virtual_path: record.source_path(),
                span: piece.span,
                message: "ZTPuzzlePiece has an invalid entityScale".to_owned(),
            })?;
        let piece_id = AssetId::from_virtual_path(&format!("fossil-pieces/{animal}/{node}"));
        piece_ids.push(piece_id);
        output.document.fossil_pieces.push(FossilPieceDefinition {
            id: piece_id,
            set,
            model: AssetId::from_virtual_path(&format!("puzzles/{animal}/{texture}")),
            slot: AssetId::from_virtual_path(&format!("fossil-slots/{animal}/{node}")),
            discovery_weight: 1,
            scale_permille: (scale * 1000.0).round().clamp(1.0, u16::MAX as f32) as u16,
        });
    }
    let pieces = piece_ids;
    output.document.fossil_sets.push(FossilSetDefinition {
        id: set,
        species: AssetId::from_virtual_path(&format!("species/{animal}")),
        pieces,
        completion_unlock: AssetId([0; 16]),
    });
    Ok(())
}

pub(super) fn bind_fossil_slot(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.fossil_slots.push(FossilSlotDefinition {
        id: id(record.key),
        transform: matrix(record)?,
        tolerance_cm: required_number(record, &["toleranceCm"])?,
        tolerance_degrees: required_number(record, &["toleranceDegrees"])?,
    });
    Ok(())
}

pub(super) fn bind_cloning_center(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let component = record
        .descendant_named("ZTCloningCenterComponent")
        .ok_or_else(|| {
            BindError::record(record, "cloning-center binder has no cloning component")
        })?;
    let failures = component
        .element_children()
        .find(|child| {
            source_document_names_are_semantically_equal(child.name.as_str(), "FailureAnimals")
        })
        .ok_or_else(|| {
            BindError::record(record, "cloning component has no FailureAnimals policy")
        })?;
    let mut failure_outcomes = Vec::new();
    for outcome in failures.element_children() {
        let definition = outcome.name.as_str().trim();
        if definition.is_empty() {
            return Err(BindError::record(
                record,
                "cloning failure outcome has no definition identity",
            ));
        }
        failure_outcomes.push(CloningFailureOutcome {
            definition: id(definition),
            test_type: id(outcome
                .attribute_named_any(&["testType"])
                .unwrap_or(definition)),
            weight: required_element_number(&outcome, &["weight"])?,
            limit: element_number(&outcome, &["limit"], 0)?,
        });
    }

    let difficulty = component
        .element_children()
        .find(|child| {
            source_document_names_are_semantically_equal(child.name.as_str(), "Difficulty")
        })
        .ok_or_else(|| BindError::record(record, "cloning component has no Difficulty policy"))?;
    let sickly_threshold = required_element_number(&difficulty, &["sicklyThreshold"])?;
    let normal_threshold = required_element_number(&difficulty, &["normalThreshold"])?;
    let super_threshold = required_element_number(&difficulty, &["superThreshold"])?;
    if !(sickly_threshold < normal_threshold && normal_threshold < super_threshold) {
        return Err(BindError::record(
            record,
            "cloning score thresholds are not strictly increasing",
        ));
    }

    let mut difficulty_levels = Vec::new();
    for level in difficulty.element_children() {
        let Some(level_number) = canonicalize_source_document_record_key(level.name.as_str())
            .strip_prefix("level")
            .and_then(|value| value.parse::<u8>().ok())
        else {
            continue;
        };
        let gestures = level
            .element_children()
            .find(|child| {
                source_document_names_are_semantically_equal(child.name.as_str(), "Gestures")
            })
            .ok_or_else(|| {
                BindError::record(
                    record,
                    format!("cloning difficulty level {level_number} has no gesture pool"),
                )
            })?;
        let mut gesture_ids = gestures
            .attributes()
            .filter_map(|(name, value)| {
                canonicalize_source_document_record_key(name)
                    .strip_prefix('g')
                    .and_then(|ordinal| ordinal.parse::<u16>().ok())
                    .map(|ordinal| (ordinal, value.trim()))
            })
            .collect::<Vec<_>>();
        gesture_ids.sort_unstable_by_key(|(ordinal, _)| *ordinal);
        if gesture_ids.is_empty()
            || gesture_ids.windows(2).any(|pair| pair[0].0 == pair[1].0)
            || gesture_ids.iter().any(|(_, value)| value.is_empty())
        {
            return Err(BindError::record(
                record,
                format!("cloning difficulty level {level_number} has an invalid gesture pool"),
            ));
        }
        let gestures = gesture_ids
            .into_iter()
            .map(|(_, value)| AssetId::from_key(value))
            .collect();
        let research_seconds: f64 = required_element_number(&level, &["researchTime"])?;
        let trace_speed: f32 = required_element_number(&level, &["traceSpeed"])?;
        let trace_speed_increment: f32 = required_element_number(&level, &["traceSpeedIncrement"])?;
        if !trace_speed.is_finite()
            || trace_speed <= 0.0
            || !trace_speed_increment.is_finite()
            || trace_speed_increment < 0.0
        {
            return Err(BindError::record(
                record,
                format!("cloning difficulty level {level_number} has invalid trace speed"),
            ));
        }
        difficulty_levels.push(CloningDifficultyLevel {
            level: level_number,
            gesture_count: required_element_number(&level, &["numGestures"])?,
            research_duration_ns: seconds_ns(research_seconds, record)?,
            trace_speed,
            trace_speed_increment,
            trace_speed_counter: required_element_number(&level, &["traceSpeedCounter"])?,
            gestures,
        });
    }
    output
        .document
        .cloning_centers
        .push(CloningCenterDefinition {
            id: id(record.key),
            object: id(record.key),
            difficulty_levels,
            failure_outcomes,
            sickly_threshold,
            normal_threshold,
            super_threshold,
        });
    Ok(())
}
