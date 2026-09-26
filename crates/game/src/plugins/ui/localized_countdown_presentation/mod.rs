use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::{localization::LocalizationFormatArgument, AssetId};

use crate::assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};

use super::{
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_node_projection_components::UiNodeId,
    localized_ui_text_writing::write_localized_ui_text as write_localized,
};
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Entity-local presentation state for one authored localized countdown.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiLocalizedCountdownPresentation {
    text_node: AssetId,
    localization_key: AssetId,
    starting_count: u32,
    current_count: u32,
    tick_interval_milliseconds: u32,
    completion_target: AssetId,
    elapsed_milliseconds: u32,
    active: bool,
}

impl UiLocalizedCountdownPresentation {
    pub(super) fn new(
        text_node: AssetId,
        localization_key: AssetId,
        starting_count: u32,
        tick_interval_milliseconds: u32,
        completion_target: AssetId,
    ) -> Self {
        Self {
            text_node,
            localization_key,
            starting_count,
            current_count: starting_count,
            tick_interval_milliseconds,
            completion_target,
            elapsed_milliseconds: 0,
            active: false,
        }
    }

    pub(super) fn set_active_and_restart(&mut self, active: bool) {
        self.active = active;
        self.elapsed_milliseconds = 0;
        self.current_count = self.starting_count;
    }
}

pub(super) fn advance_localized_countdown_presentations_and_activate_completion_targets(
    time: Res<Time<Real>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut countdowns: Query<(&UiDocumentOwner, &mut UiLocalizedCountdownPresentation)>,
    nodes: Query<(Entity, &UiNodeId, &UiDocumentOwner)>,
    mut texts: Query<(&UiNodeId, &UiDocumentOwner, &mut Text)>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    let elapsed_milliseconds = time.delta().as_millis().min(u128::from(u32::MAX)) as u32;
    for (owner, mut countdown) in &mut countdowns {
        if !countdown.active {
            continue;
        }

        let write_current_count =
            |countdown: &UiLocalizedCountdownPresentation,
             texts: &mut Query<(&UiNodeId, &UiDocumentOwner, &mut Text)>| {
                if let (Some(localization), Some((_, _, mut text))) = (
                    localization,
                    texts.iter_mut().find(|(id, text_owner, _)| {
                        (text_owner.0, id.id) == (owner.0, countdown.text_node)
                    }),
                ) {
                    let _ = write_localized(
                        localization,
                        countdown.localization_key,
                        &[LocalizationFormatArgument::Integer(i64::from(
                            countdown.current_count,
                        ))],
                        &mut text.0,
                    );
                }
            };

        if countdown.elapsed_milliseconds == 0 {
            write_current_count(&countdown, &mut texts);
        }
        countdown.elapsed_milliseconds = countdown
            .elapsed_milliseconds
            .saturating_add(elapsed_milliseconds);
        while countdown.elapsed_milliseconds >= countdown.tick_interval_milliseconds {
            countdown.elapsed_milliseconds -= countdown.tick_interval_milliseconds;
            if countdown.current_count > 1 {
                countdown.current_count -= 1;
                write_current_count(&countdown, &mut texts);
                continue;
            }
            if let Some((entity, _, _)) = nodes.iter().find(|(_, id, target_owner)| {
                (target_owner.0, id.id) == (owner.0, countdown.completion_target)
            }) {
                activated.write(UiNodeActivated {
                    source: crate::plugins::input::input_types::ActionSource::System,
                    node: entity,
                    trigger: UiTrigger::Press,
                });
            }
            countdown.active = false;
            break;
        }
    }
}
