//! Presentation records for timed UI events.

use crate::assets::source_document::ui::model::SourceUiTimedEvent;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_event_action_lowering;
use crate::assets::ui_document::source::lower::authored_ui_event_collection_lowering::inherited_child;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::timed_presentation::{
    UiCreditsCardRecord, UiLocalizedCountdownRecord, UiTimedEventRecord,
};
use openzt2_game_data::ui_document::widget::UiWidgetRecord;
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;
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
    if events.len() == 37 {
        let record =
            lower_authored_localized_countdown_to_canonical_record(events, role, output, input)?;
        return Ok(UiWidgetRecord::LocalizedCountdown(record));
    }
    match name.unwrap_or_default() {
        "credits" => {
            lower_authored_credits_sequence_to_canonical_widget_record(events, role, input)
        }
        "EarthQuake"
            if events.len() == 5
                && events
                    .iter()
                    .zip([0.0, 1.0, 0.0, 6.0, 0.0])
                    .all(|(event, delay)| event.delay_seconds == delay)
                && events[0].event.message == "UI_PLAY_SOUND"
                && events[0].event.string.as_deref() == Some("earthquake")
                && events[1].event.message == "ZT_RUN_SCRIPT"
                && events[1].event.string.as_deref()
                    == Some("global scenario/scripts/earthquake.lua runQuake")
                && events[2].event.message == "UI_CHILD"
                && events[2].event.target_child.as_deref() == Some("QuakeRunner")
                && events[2]
                    .event
                    .child
                    .as_deref()
                    .map(|event| event.message.as_str())
                    == Some("UI_ACTIVATE_ON")
                && events[3].event.message == "UI_CHILD"
                && events[3].event.target_child.as_deref() == Some("QuakeRunner")
                && events[3]
                    .event
                    .child
                    .as_deref()
                    .map(|event| event.message.as_str())
                    == Some("UI_ACTIVATE_OFF")
                && events[4].event.message == "UI_SELF"
                && events[4]
                    .event
                    .child
                    .as_deref()
                    .map(|event| event.message.as_str())
                    == Some("UI_ACTIVATE_OFF") =>
        {
            Ok(UiWidgetRecord::EarthquakeTrigger)
        }
        "QuakeRunner"
            if events.len() == 2
                && events.iter().all(|event| event.delay_seconds == 0.0)
                && events[0].event.message == "ZT_RUN_SCRIPT"
                && events[0].event.string.as_deref()
                    == Some("global scenario/scripts/earthquake.lua updateQuake")
                && events[1].event.message == "UI_SELF"
                && events[1]
                    .event
                    .child
                    .as_deref()
                    .map(|event| event.message.as_str())
                    == Some("UI_ACTIVATE_ON") =>
        {
            Ok(UiWidgetRecord::EarthquakeRunner)
        }
        "count down" => {
            lower_authored_first_person_countdown_to_canonical_widget_record(events, role, input)
        }
        "Buy_Vehicle_Pulser" | "Buy Upgrade Pulser" => {
            lower_authored_button_pulser_to_canonical_widget_record(events, role, input)
        }
        _ => lower_authored_timed_sequence_to_canonical_widget_record(
            current, events, role, output, input,
        ),
    }
}

fn lower_authored_timed_sequence_to_canonical_widget_record(
    current: AssetId,
    events: &[SourceUiTimedEvent],
    role: UiDocumentRole,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<UiWidgetRecord> {
    let mut timed_events = Vec::new();
    let mut due_ms = 0u32;
    for timed in events {
        due_ms = due_ms
            .checked_add(convert_authored_presentation_delay_seconds_to_milliseconds(
                timed.delay_seconds,
                input,
            )?)
            .ok_or_else(|| invalid_at(input, "timed UI sequence duration overflow"))?;
        let event = if timed.event.message == "UI_CHILD" && timed.event.child.is_some() {
            inherited_child(&timed.event, input)?
        } else {
            timed.event.clone()
        };
        let Some(UiActionRecord::Presentation(action)) =
            authored_ui_event_action_lowering::lower_authored_ui_event_to_canonical_action(
                UiTrigger::Press,
                &event,
                role,
                current,
                "",
                current,
                output,
                input,
                None,
            )?
        else {
            return Err(invalid_at(
                input,
                "non-visual timed UI action requires its canonical domain clock and owner",
            ));
        };
        timed_events.push(UiTimedEventRecord { due_ms, action });
    }
    Ok(UiWidgetRecord::TimedSequence {
        events: timed_events,
    })
}

fn lower_authored_first_person_countdown_to_canonical_widget_record(
    events: &[SourceUiTimedEvent],
    role: UiDocumentRole,
    input: &AuthoredUiDocument,
) -> io::Result<UiWidgetRecord> {
    let targets = [
        "Counter layout 3",
        "Counter layout 2",
        "Counter layout 1",
        "Counter layout go",
    ];
    if events.len() != 9 {
        return Err(invalid_at(
            input,
            "first-person countdown has an unknown shape",
        ));
    }
    let expected_delays = [0.1, 0.5, 0.1, 0.5, 0.1, 0.5, 0.1, 0.05, 0.5];
    if !events
        .iter()
        .zip(expected_delays)
        .all(|(event, delay)| event.delay_seconds == delay)
    {
        return Err(invalid_at(
            input,
            "first-person countdown timing is unknown",
        ));
    }
    for (pair, target) in targets.iter().enumerate() {
        let show = &events[pair * 2];
        let hide_index = if pair == 3 { 8 } else { pair * 2 + 1 };
        let hide = &events[hide_index];
        if show.event.message != "UI_CHILD"
            || show.event.target_child.as_deref() != Some(*target)
            || show
                .event
                .child
                .as_deref()
                .map(|event| event.message.as_str())
                != Some("UI_SHOW")
            || hide.event.message != "UI_CHILD"
            || hide.event.target_child.as_deref() != Some(*target)
            || hide
                .event
                .child
                .as_deref()
                .map(|event| event.message.as_str())
                != Some("UI_HIDE")
        {
            return Err(invalid_at(
                input,
                "first-person countdown has unknown presentation targets",
            ));
        }
    }
    if events[7].event.message != "ZT_FP_TRACINGSTART" {
        return Err(invalid_at(
            input,
            "first-person countdown has no typed completion",
        ));
    }
    Ok(UiWidgetRecord::FirstPersonCountdown {
        three: UiDocumentRole::node_id(role, targets[0]),
        two: UiDocumentRole::node_id(role, targets[1]),
        one: UiDocumentRole::node_id(role, targets[2]),
        go: UiDocumentRole::node_id(role, targets[3]),
    })
}

fn lower_authored_button_pulser_to_canonical_widget_record(
    events: &[SourceUiTimedEvent],
    role: UiDocumentRole,
    input: &AuthoredUiDocument,
) -> io::Result<UiWidgetRecord> {
    if events.len() != 3
        || events[0].event.message != "UI_CHILD"
        || events[0].delay_seconds != 0.0
        || events[1].delay_seconds != 0.19
        || events[2].delay_seconds != 1.0
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
        return Err(invalid_at(input, "button pulser has an unknown shape"));
    }
    Ok(UiWidgetRecord::ButtonPulser {
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
    })
}

fn lower_authored_credits_sequence_to_canonical_widget_record(
    events: &[SourceUiTimedEvent],
    role: UiDocumentRole,
    input: &AuthoredUiDocument,
) -> io::Result<UiWidgetRecord> {
    let mut credits_cards = Vec::new();
    let mut elapsed = 0u32;
    let mut open = BTreeMap::<[u8; 16], usize>::new();
    let mut repeat_after_ms = 0;
    let mut suspend_rail_camera = false;
    for timed in events {
        elapsed = elapsed
            .checked_add(convert_authored_presentation_delay_seconds_to_milliseconds(
                timed.delay_seconds,
                input,
            )?)
            .ok_or_else(|| invalid_at(input, "credits sequence duration overflow"))?;
        let event = &timed.event;
        if event.message == "UI_ACTIVATE_ON" {
            repeat_after_ms = elapsed;
            continue;
        }
        if event.message != "UI_CHILD" {
            return Err(invalid_at(
                input,
                "credits sequence contains a non-credits action",
            ));
        }
        let target = event.target_child.as_deref().unwrap_or_default();
        let child = event
            .child
            .as_deref()
            .map(|child| child.message.as_str())
            .unwrap_or_default();
        if target == "railcam" && matches!(child, "UI_ACTIVATE_OFF" | "UI_ACTIVATE_ON") {
            suspend_rail_camera = true;
            continue;
        }
        let id = UiDocumentRole::node_id(role, target);
        match child {
            "UI_SHOW" => {
                let index = credits_cards.len();
                credits_cards.push(UiCreditsCardRecord {
                    node: id,
                    show_at_ms: elapsed,
                    hide_at_ms: u32::MAX,
                    wind_after_hide: false,
                });
                open.insert(id.0, index);
            }
            "UI_HIDE" => {
                let index = open.remove(&id.0).ok_or_else(|| {
                    invalid_at(
                        input,
                        format!("credits hide has no preceding show for {target:?}"),
                    )
                })?;
                credits_cards[index].hide_at_ms = elapsed;
            }
            "UI_WIND_ANIMATION" => {
                let index = credits_cards
                    .iter()
                    .rposition(|card| card.node == id)
                    .ok_or_else(|| {
                        invalid_at(
                            input,
                            format!("credits wind has no presented card for {target:?}"),
                        )
                    })?;
                credits_cards[index].wind_after_hide = true;
            }
            _ => {
                return Err(invalid_at(
                    input,
                    format!("credits sequence has unknown focused operation {child:?}"),
                ));
            }
        }
    }
    Ok(UiWidgetRecord::CreditsSequence {
        cards: credits_cards,
        repeat_after_ms,
        suspend_rail_camera,
    })
}

fn lower_authored_localized_countdown_to_canonical_record(
    events: &[SourceUiTimedEvent],
    role: UiDocumentRole,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<UiLocalizedCountdownRecord> {
    if events.len() != 37 {
        return Err(invalid_at(
            input,
            "timed UI widget is not the evidenced 36-second localized countdown",
        ));
    }
    let mut text_node = None;
    let mut key = None;
    for (index, timed) in events[..36].iter().enumerate() {
        let expected_count = 36 - index as u32;
        let expected_delay = if index == 0 { 0.0 } else { 1.0 };
        if timed.event.message != "ZT_RUN_SCRIPT" || timed.delay_seconds != expected_delay {
            return Err(invalid_at(
                input,
                "localized countdown has an unexpected update cadence",
            ));
        }
        let words = timed
            .event
            .string
            .as_deref()
            .unwrap_or_default()
            .split_ascii_whitespace()
            .collect::<Vec<_>>();
        let ["global", "scripts/uiutil.lua", "setReplacementText", node, localization, count] =
            words.as_slice()
        else {
            return Err(invalid_at(
                input,
                "localized countdown update has an unknown source shape",
            ));
        };
        if count.parse::<u32>().ok() != Some(expected_count) {
            return Err(invalid_at(
                input,
                "localized countdown values are not the evidenced 36 through 1 sequence",
            ));
        }
        let node = UiDocumentRole::node_id(role, node);
        let localization = AssetId::from_key(localization);
        if text_node
            .replace(node)
            .is_some_and(|previous| previous != node)
            || key
                .replace(localization)
                .is_some_and(|previous| previous != localization)
        {
            return Err(invalid_at(
                input,
                "localized countdown changes target or localization key",
            ));
        }
    }
    let completion = &events[36];
    if completion.delay_seconds != 1.0
        || completion.event.message != "UI_CHILD"
        || completion
            .event
            .child
            .as_deref()
            .map(|child| child.message.as_str())
            != Some("UI_ACTIVATE")
    {
        return Err(invalid_at(
            input,
            "localized countdown has an unknown completion action",
        ));
    }
    let localization_key =
        key.ok_or_else(|| invalid_at(input, "localized countdown has no key"))?;
    output.dependencies.insert(localization_key.0);
    Ok(UiLocalizedCountdownRecord {
        text_node: text_node
            .ok_or_else(|| invalid_at(input, "localized countdown has no text target"))?,
        localization_key,
        start_count: 36,
        tick_interval_ms: 1_000,
        completion_target: UiDocumentRole::node_id(
            role,
            completion.event.target_child.as_deref().unwrap_or_default(),
        ),
    })
}
