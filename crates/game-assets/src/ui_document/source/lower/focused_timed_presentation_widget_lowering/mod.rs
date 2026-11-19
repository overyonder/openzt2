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
use openzt2_game_data::ui_document::timed_presentation::{UiTimedEventAction, UiTimedEventRecord};
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
        let mut message = event;
        while let Some(child) = message.child.as_deref().filter(|_| {
            matches!(
                message.message.as_str(),
                "UI_CHILD" | "UI_ROOT_CHILD" | "UI_SELF"
            )
        }) {
            message = child;
        }
        let script = (message.message == "ZT_RUN_SCRIPT")
            .then_some(message.string.as_deref())
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
                action: UiTimedEventAction::Ui(UiActionRecord::Presentation(
                    UiPresentationActionRecord {
                        trigger: UiTrigger::Press,
                        action: UiPresentationAction::FormatTargetNodeLocalizedInteger {
                            target_node: UiDocumentRole::node_id(role, node),
                            localization_key: AssetId::from_key(localization),
                            value,
                        },
                    },
                )),
            });
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
        if output.actions.len() == first_action {
            if let Some(script) = script {
                actions.push(UiTimedEventRecord {
                    due_ms,
                    action: UiTimedEventAction::ScenarioScript(script.to_owned()),
                });
            }
        }
        for action in output.actions.drain(first_action..) {
            actions.push(UiTimedEventRecord {
                due_ms,
                action: UiTimedEventAction::Ui(action),
            });
        }
    }
    // A self-restarting visibility sequence is an ambient UI effect. Script
    // sequences and an otherwise empty self-restart wait for explicit activation.
    let starts_active = actions.iter().any(|entry| {
        matches!(
            &entry.action,
            UiTimedEventAction::Ui(UiActionRecord::Presentation(UiPresentationActionRecord {
                action: UiPresentationAction::SetTargetNodeVisible { .. }
                    | UiPresentationAction::SetDocumentNodeVisible { .. },
                ..
            }))
        )
    }) && events.last().is_some_and(|event| {
        event.event.message == "UI_ACTIVATE_ON"
            && event.event.target_child.is_none()
            && event.event.string.is_none()
            && event.event.value.is_none()
    });
    Ok(UiWidgetRecord::TimedSequence {
        events: actions,
        starts_active,
    })
}
