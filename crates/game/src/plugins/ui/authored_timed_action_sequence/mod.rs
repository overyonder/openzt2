use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::widget::UiWidgetRecord;

use super::authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot};
use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::authored_ui_activation_contracts::UiNodeActivated,
};

/// Entity-local presentation clock and cursor for one loaded authored timed
/// action sequence. Event records remain borrowed from the UI document.
#[derive(Component, Debug, Clone, PartialEq)]
pub(super) struct UiAuthoredTimedActionSequencePlayback {
    authored_node_index: u32,
    action_proxy_entities: Vec<Entity>,
    elapsed_milliseconds: f64,
    next_event_index: usize,
    active: bool,
}

impl UiAuthoredTimedActionSequencePlayback {
    pub(super) fn new(authored_node_index: u32, action_proxy_entities: Vec<Entity>) -> Self {
        Self {
            authored_node_index,
            action_proxy_entities,
            elapsed_milliseconds: 0.0,
            next_event_index: 0,
            active: false,
        }
    }

    pub(super) fn set_active_and_restart(&mut self, active: bool) {
        self.active = active;
        self.elapsed_milliseconds = 0.0;
        self.next_event_index = 0;
    }
}

/// Advances authored timed sequences on Bevy's real-time clock and activates
/// their statically typed action proxies in source order.
pub(super) fn advance_authored_timed_action_sequences_and_activate_due_proxies(
    time: Res<Time<Real>>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut sequences: Query<(&UiDocumentOwner, &mut UiAuthoredTimedActionSequencePlayback)>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let elapsed_milliseconds = time.delta_secs_f64() * 1_000.0;
    for (owner, mut sequence) in &mut sequences {
        if !sequence.active {
            continue;
        }
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        sequence.elapsed_milliseconds += elapsed_milliseconds;
        let Some(UiWidgetRecord::TimedSequence { events }) = document
            .canonical_ui_document()
            .nodes
            .get(sequence.authored_node_index as usize)
            .map(|node| &node.widget)
        else {
            sequence.active = false;
            continue;
        };
        while sequence.next_event_index < events.len() {
            let Some(event) = events.get(sequence.next_event_index) else {
                sequence.active = false;
                break;
            };
            if f64::from(event.due_ms) > sequence.elapsed_milliseconds {
                break;
            }
            if let Some(&proxy) = sequence
                .action_proxy_entities
                .get(sequence.next_event_index)
            {
                activated.write(UiNodeActivated {
                    source: crate::plugins::input::input_types::ActionSource::System,
                    node: proxy,
                    trigger: UiTrigger::Press,
                });
            }
            sequence.next_event_index += 1;
        }
        if sequence.next_event_index == events.len() {
            sequence.active = false;
        }
    }
}
