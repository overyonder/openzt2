//! Presentation records for timed UI events.

use crate::source_document::ui::model::SourceUiTimedEvent;
use crate::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::action::presentation::{
    UiPresentationAction, UiPresentationActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::timed_presentation::UiTimedEventRecord;
use openzt2_game_data::ui_document::widget::UiWidgetRecord;
use openzt2_game_data::AssetId;
use std::io;

fn convert_authored_presentation_delay_seconds_to_milliseconds(
    seconds: f32,
    input: &AuthoredUiDocument,
) -> io::Result<u32> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(invalid_at(input, "timed presentation delay is invalid"));
    }
    u32::try_from((seconds * 1_000.0).round() as u64)
        .map_err(|_| invalid_at(input, "timed presentation delay is too large"))
}

pub(super) fn lower_source_ui_timed_events_to_canonical_focused_presentation_widget_record(
    name: Option<&str>,
    current: AssetId,
    events: &[SourceUiTimedEvent],
    role: UiDocumentRole,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<UiWidgetRecord> {
    if matches!(name, Some("Buy_Vehicle_Pulser" | "Buy Upgrade Pulser")) {
        if let Some(pulser) =
            lower_authored_button_pulser_to_canonical_widget_record(events, role, input)?
        {
            return Ok(pulser);
        }
    }
    let mut actions = Vec::with_capacity(events.len());
    let mut due_ms = 0u32;
    for timed in events {
        due_ms = due_ms
            .checked_add(convert_authored_presentation_delay_seconds_to_milliseconds(
                timed.delay_seconds,
                input,
            )?)
            .ok_or_else(|| invalid_at(input, "timed UI sequence duration overflow"))?;
        let event = &timed.event;
        let script = (event.message == "ZT_RUN_SCRIPT")
            .then_some(event.string.as_deref())
            .flatten();
        let words = script.map(|script| script.split_ascii_whitespace().collect::<Vec<_>>());
        if let Some(
            ["global", "scripts/uiutil.lua", "setReplacementText", node, localization, count],
        ) = words.as_deref()
        {
            let value = count
                .parse::<i64>()
                .map_err(|_| invalid_at(input, "replacement text requires an integer"))?;
            actions.push(UiTimedEventRecord {
                due_ms,
                action: UiActionRecord::Presentation(UiPresentationActionRecord {
                    trigger: UiTrigger::Press,
                    action: UiPresentationAction::FormatTargetNodeLocalizedInteger {
                        target_node: UiDocumentRole::node_id(role, node),
                        localization_key: AssetId::from_key(localization),
                        value,
                    },
                }),
            });
            continue;
        }
        // The scenario owns the recurring tutorial ring callback and its clock.
        if matches!(name, Some("ring timer"))
            && script.is_some_and(|script| script.ends_with(" updateTutorialRing"))
        {
            continue;
        }
        let first_action = output.actions.len();
        super::authored_ui_event_collection_lowering::flatten_event(
            UiTrigger::Press,
            event,
            role,
            current,
            name.unwrap_or_default(),
            current,
            None,
            output,
            input,
            None,
            &[],
        )?;
        for action in output.actions.drain(first_action..) {
            actions.push(UiTimedEventRecord { due_ms, action });
        }
    }
    Ok(UiWidgetRecord::TimedSequence { events: actions })
}

fn lower_authored_button_pulser_to_canonical_widget_record(
    events: &[SourceUiTimedEvent],
    role: UiDocumentRole,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiWidgetRecord>> {
    if events.len() != 3
        || events[0].event.message != "UI_CHILD"
        || events[0].delay_seconds != 0.0
        || events[0]
            .event
            .child
            .as_deref()
            .map(|event| event.message.as_str())
            != Some("UI_HIDE")
        || events[1].event.message != "UI_CHILD"
        || events[1].event.target_child != events[0].event.target_child
        || events[1]
            .event
            .child
            .as_deref()
            .map(|event| event.message.as_str())
            != Some("UI_SHOW")
        || events[2].event.message != "UI_ACTIVATE_ON"
    {
        return Ok(None);
    }
    Ok(Some(UiWidgetRecord::ButtonPulser {
        target: UiDocumentRole::node_id(
            role,
            events[0].event.target_child.as_deref().unwrap_or_default(),
        ),
        hidden_ms: convert_authored_presentation_delay_seconds_to_milliseconds(
            events[1].delay_seconds,
            input,
        )?,
        visible_ms: convert_authored_presentation_delay_seconds_to_milliseconds(
            events[2].delay_seconds,
            input,
        )?,
    }))
}
