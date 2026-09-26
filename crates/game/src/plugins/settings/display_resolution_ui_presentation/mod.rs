use bevy::{
    prelude::*,
    window::{Monitor, PrimaryMonitor},
};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;

use crate::plugins::ui::{
    authored_reusable_list_and_table_runtime_types::{SetUiListRowCount, UiListPolicy, UiListRow},
    authored_ui_selection_state::UiSelected,
};

use super::{
    display_settings_types::{
        DisplayResolution, DisplayResolutionChoice, SupportedDisplayResolutions,
    },
    options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
};
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn synchronize_supported_display_resolutions_from_primary_monitor(
    changed: Query<(), Or<(Added<Monitor>, Changed<Monitor>)>>,
    monitors: Query<(&Monitor, Option<&PrimaryMonitor>)>,
    mut supported: ResMut<SupportedDisplayResolutions>,
) {
    if changed.is_empty() {
        return;
    }
    let selected = monitors
        .iter()
        .find_map(|(monitor, primary)| primary.is_some().then_some(monitor))
        .or_else(|| monitors.iter().next().map(|(monitor, _)| monitor));
    let mut resolutions = selected.map_or_else(Vec::new, |monitor| {
        monitor
            .video_modes
            .iter()
            .map(|mode| DisplayResolution {
                width: mode.physical_size.x,
                height: mode.physical_size.y,
            })
            .collect::<Vec<_>>()
    });
    resolutions.sort_unstable();
    resolutions.dedup();
    if supported.0 != resolutions {
        supported.0 = resolutions;
    }
}

/// Requests one authored `ResolutionEntry` row per renderer mode. Asset loading
/// has linked the options list to that source document, so the game supplies
/// only the live monitor facts.
pub(super) fn set_authored_display_resolution_list_row_count(
    supported: Res<SupportedDisplayResolutions>,
    lists: Query<(Entity, Ref<UiListPolicy>)>,
    mut row_counts: MessageWriter<SetUiListRowCount>,
) {
    for (list, policy) in &lists {
        if policy.source == UiWidgetLiveCollectionSource::DisplayResolutions
            && (supported.is_changed() || policy.is_added())
        {
            row_counts.write(SetUiListRowCount {
                list,
                count: u16::try_from(supported.0.len()).unwrap_or(u16::MAX),
            });
        }
    }
}

/// Hydrates authored resolution rows with their monitor identity and label.
/// Typography, layout, visuals, and hit policy remain those of
/// `UI/layout/resolution.xml`.
pub(super) fn project_supported_display_resolutions_into_authored_rows(
    mut commands: Commands,
    supported: Res<SupportedDisplayResolutions>,
    rows: Query<(Entity, Ref<UiListRow>, Option<&DisplayResolutionChoice>)>,
    lists: Query<&UiListPolicy>,
    parents: Query<&ChildOf>,
    mut texts: Query<(Entity, &mut Text)>,
) {
    for (row_entity, row, current) in &rows {
        let Ok(policy) = lists.get(row.list) else {
            continue;
        };
        if policy.source != UiWidgetLiveCollectionSource::DisplayResolutions
            || (!supported.is_changed() && !row.is_added())
        {
            continue;
        }
        let Some(resolution) = supported.0.get(usize::from(row.index)).copied() else {
            continue;
        };
        let choice = DisplayResolutionChoice(resolution);
        if current != Some(&choice) {
            commands.entity(row_entity).insert(choice);
        }
        let label = format!("{} x {}", resolution.width, resolution.height);
        for (entity, mut text) in &mut texts {
            if entity_is_descendant_of_or_equal_to(entity, row_entity, &parents) && text.0 != label
            {
                text.0.clone_from(&label);
            }
        }
    }
}

/// Keeps mode selection local to the options draft. The accepted window is
/// unchanged until the authored accept action emits `ReplaceDisplaySettingsRequest`.
pub(super) fn copy_activated_display_resolution_into_settings_draft(
    mut activations: MessageReader<UiNodeActivated>,
    choices: Query<&DisplayResolutionChoice>,
    parents: Query<&ChildOf>,
    mut draft: ResMut<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
) {
    for activation in activations
        .read()
        .filter(|activation| matches!(activation.trigger, UiTrigger::Press | UiTrigger::Submit))
    {
        let choice = std::iter::successors(Some(activation.node), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .find_map(|entity| choices.get(entity).ok());
        if let Some(choice) = choice {
            if draft.display.width != choice.0.width || draft.display.height != choice.0.height {
                draft.display.width = choice.0.width;
                draft.display.height = choice.0.height;
                draft.dirty_display = true;
            }
        }
    }
}

pub(super) fn project_drafted_display_resolution_selection_into_authored_rows(
    draft: Res<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
    choices: Query<(Entity, Ref<DisplayResolutionChoice>)>,
    parents: Query<&ChildOf>,
    mut selected: Query<(Entity, &mut UiSelected)>,
) {
    if !draft.is_changed() && choices.iter().all(|(_, choice)| !choice.is_added()) {
        return;
    }
    for (row, choice) in &choices {
        if !draft.is_changed() && !choice.is_added() {
            continue;
        }
        let next = draft.display.width == choice.0.width && draft.display.height == choice.0.height;
        for (entity, mut selected) in &mut selected {
            if entity_is_descendant_of_or_equal_to(entity, row, &parents) && selected.0 != next {
                selected.0 = next;
            }
        }
    }
}

fn entity_is_descendant_of_or_equal_to(
    entity: Entity,
    ancestor: Entity,
    parents: &Query<&ChildOf>,
) -> bool {
    std::iter::successors(Some(entity), |entity| {
        parents.get(*entity).ok().map(ChildOf::parent)
    })
    .any(|entity| entity == ancestor)
}
