use bevy::{
    prelude::*,
    window::{Monitor, PrimaryMonitor},
};
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;

use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::{
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_selection_state::UiSelected,
};

use super::{
    display_settings_types::{
        DisplayResolution, DisplayResolutionChoice, SupportedDisplayResolutions,
    },
    options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
};
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

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

pub(super) fn project_supported_display_resolutions_into_authored_rows(
    mut commands: Commands,
    supported: Res<SupportedDisplayResolutions>,
    mut rows: Query<(
        Entity,
        Ref<UiListRow>,
        Option<&mut DisplayResolutionChoice>,
        &ChildOf,
    )>,
    lists: Query<&UiListPolicy>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
) {
    for (entity, row, current, parent) in &mut rows {
        if !supported.is_changed() && !row.is_changed() {
            continue;
        }
        if !lists
            .get(parent.parent())
            .is_ok_and(|policy| policy.source == UiWidgetLiveCollectionSource::DisplayResolutions)
        {
            continue;
        }
        let Some(&resolution) = supported.0.get(usize::from(row.index)) else {
            continue;
        };
        let choice = DisplayResolutionChoice(resolution);
        if let Some(mut current) = current {
            current.set_if_neq(choice);
        } else {
            commands.entity(entity).insert(choice);
        }
        let label = format!("{} x {}", resolution.width, resolution.height);
        for descendant in std::iter::once(entity).chain(children.iter_descendants(entity)) {
            if let Ok(mut text) = texts.get_mut(descendant) {
                if text.0 != label {
                    text.0.clone_from(&label);
                }
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
    mut drafts: Query<&mut OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
) {
    for activation in activations
        .read()
        .filter(|activation| matches!(activation.trigger, UiTrigger::Press | UiTrigger::Submit))
    {
        let ancestors = || {
            std::iter::once(activation.node)
                .chain(parents.iter_ancestors::<ChildOf>(activation.node))
        };
        let choice = ancestors().find_map(|entity| choices.get(entity).ok());
        let Some(draft_owner) = ancestors().find(|entity| drafts.contains(*entity)) else {
            continue;
        };
        let Ok(mut draft) = drafts.get_mut(draft_owner) else {
            continue;
        };
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
    drafts: Query<Ref<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>>,
    choices: Query<(Entity, Ref<DisplayResolutionChoice>)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    mut selected: Query<&mut UiSelected>,
) {
    for (row, choice) in &choices {
        let Some(draft) = parents
            .iter_ancestors::<ChildOf>(row)
            .find_map(|entity| drafts.get(entity).ok())
        else {
            continue;
        };
        if !draft.is_changed() && !choice.is_changed() {
            continue;
        }
        let next = draft.display.width == choice.0.width && draft.display.height == choice.0.height;
        for entity in std::iter::once(row).chain(children.iter_descendants(row)) {
            if let Ok(mut selected) = selected.get_mut(entity) {
                selected.set_if_neq(UiSelected(next));
            }
        }
    }
}

/// Shows the drafted mode in the options screen's `Video Setting` field, as
/// the source options mode does above its resolution list.
pub(super) fn project_drafted_display_resolution_into_authored_text(
    drafts: Query<Ref<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>>,
    mut texts: Query<(Ref<UiTextBinding>, &UiDocumentOwner, &mut Text)>,
) {
    for (binding, owner, text) in &mut texts {
        if binding.0 != UiTextPropertyBindingSource::DraftDisplayResolution {
            continue;
        }
        let Ok(draft) = drafts.get(owner.0) else {
            continue;
        };
        if draft.is_changed() || binding.is_added() {
            text.map_unchanged(|text| &mut text.0).set_if_neq(format!(
                "{} x {}",
                draft.display.width, draft.display.height
            ));
        }
    }
}
