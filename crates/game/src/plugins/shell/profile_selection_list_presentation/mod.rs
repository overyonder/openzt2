use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    node_property_binding::*, widget_live_collection::UiWidgetLiveCollectionSource,
};

use crate::plugins::{
    persistence::profile_types::{ProfileIndex, ProfileIndexReady, ProfileSelected},
    ui::{
        authored_reusable_list_and_table_runtime_types::{
            SetUiListRowCount, UiListPolicy, UiListRow,
        },
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
        authored_ui_node_projection_components::UiValue,
        authored_ui_text_content_binding::UiTextBinding,
    },
};

use super::shell_selection_types::{ProfileChoice, ShellScreen};

/// Resizes the profile list after the index loads or the list appears.
pub(super) fn request_authored_profile_selection_list_row_count(
    mut ready: MessageReader<ProfileIndexReady>,
    index: Res<ProfileIndex>,
    lists: Query<(Entity, Ref<UiListPolicy>)>,
    mut rows: MessageWriter<SetUiListRowCount>,
) {
    let index_ready = ready.read().next().is_some();
    for (list, policy) in &lists {
        if policy.source == UiWidgetLiveCollectionSource::ProfileIndex
            && (index_ready || index.is_changed() || policy.is_added())
        {
            rows.write(SetUiListRowCount {
                list,
                count: index.profile_records.len().min(u16::MAX as usize) as u16,
            });
        }
    }
}

/// Gives each row its profile identity and label. Child controls receive the
/// index used by their rename and delete actions.
pub(super) fn project_profile_index_entries_into_authored_rows(
    mut commands: Commands,
    index: Res<ProfileIndex>,
    rows: Query<(Entity, Ref<UiListRow>)>,
    lists: Query<&UiListPolicy>,
    children: Query<&Children>,
    bindings: Query<&UiTextBinding>,
    mut texts: Query<&mut Text>,
) {
    for (row_entity, row) in &rows {
        if !lists
            .get(row.list)
            .is_ok_and(|policy| policy.source == UiWidgetLiveCollectionSource::ProfileIndex)
        {
            continue;
        }
        if !index.is_changed() && !row.is_added() && !row.is_changed() {
            continue;
        }
        let Some(profile) = index.profile_records.get(usize::from(row.index)) else {
            commands
                .entity(row_entity)
                .remove::<(ProfileChoice, UiValue)>();
            commands.entity(row_entity).insert(Visibility::Hidden);
            continue;
        };
        commands.entity(row_entity).insert((
            ProfileChoice(profile.profile_identifier),
            Visibility::Inherited,
        ));
        project_profile_index_entry_into_authored_row_subtree(
            &mut commands,
            row_entity,
            row.index,
            &profile.profile_display_name,
            &children,
            &bindings,
            &mut texts,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn project_profile_index_entry_into_authored_row_subtree(
    commands: &mut Commands,
    entity: Entity,
    index: u16,
    display_name: &str,
    children: &Query<&Children>,
    bindings: &Query<&UiTextBinding>,
    texts: &mut Query<&mut Text>,
) {
    commands.entity(entity).insert(UiValue(i64::from(index)));
    if bindings
        .get(entity)
        .is_ok_and(|binding| binding.0 == UiTextPropertyBindingSource::ProfileName)
    {
        if let Ok(mut text) = texts.get_mut(entity) {
            if text.0 != display_name {
                text.0.clear();
                text.0.push_str(display_name);
            }
        }
    }
    let Ok(entity_children) = children.get(entity) else {
        return;
    };
    for child in entity_children.iter() {
        project_profile_index_entry_into_authored_row_subtree(
            commands,
            child,
            index,
            display_name,
            children,
            bindings,
            texts,
        );
    }
}

/// Fills the selected profile's name when the main-menu text node appears.
pub(super) fn project_selected_profile_name_into_new_main_menu_nodes(
    index: Res<ProfileIndex>,
    mut nodes: Query<(&UiTextBinding, &UiDocumentOwner, &mut Text), Added<UiTextBinding>>,
    roots: Query<&ChildOf, With<UiDocumentRoot>>,
    screens: Query<&ShellScreen>,
) {
    let Some(display_name) = selected_profile_display_name(&index) else {
        return;
    };
    for (binding, owner, text) in &mut nodes {
        if binding.0 == UiTextPropertyBindingSource::ProfileName
            && ui_document_owner_belongs_to_main_menu_screen(*owner, &roots, &screens)
        {
            write_selected_profile_name_into_authored_text(text, display_name);
        }
    }
}

/// Refreshes the same authored node only when persistence reports a completed
/// load or selection. There is no per-frame profile catalogue scan.
pub(super) fn refresh_selected_profile_name_after_profile_index_changes(
    mut ready: MessageReader<ProfileIndexReady>,
    mut selected: MessageReader<ProfileSelected>,
    index: Res<ProfileIndex>,
    mut nodes: Query<(&UiTextBinding, &UiDocumentOwner, &mut Text)>,
    roots: Query<&ChildOf, With<UiDocumentRoot>>,
    screens: Query<&ShellScreen>,
) {
    let changed = ready.read().next().is_some() | selected.read().next().is_some();
    if !changed {
        return;
    }
    let Some(display_name) = selected_profile_display_name(&index) else {
        return;
    };
    for (binding, owner, text) in &mut nodes {
        if binding.0 == UiTextPropertyBindingSource::ProfileName
            && ui_document_owner_belongs_to_main_menu_screen(*owner, &roots, &screens)
        {
            write_selected_profile_name_into_authored_text(text, display_name);
        }
    }
}

fn selected_profile_display_name(profile_index: &ProfileIndex) -> Option<&str> {
    let selected = profile_index.selected_profile_identifier?;
    profile_index
        .profile_records
        .iter()
        .find(|profile| profile.profile_identifier == selected)
        .map(|profile| profile.profile_display_name.as_str())
}

fn ui_document_owner_belongs_to_main_menu_screen(
    owner: UiDocumentOwner,
    roots: &Query<&ChildOf, With<UiDocumentRoot>>,
    screens: &Query<&ShellScreen>,
) -> bool {
    roots
        .get(owner.0)
        .ok()
        .and_then(|parent| screens.get(parent.parent()).ok())
        .is_some_and(|screen| *screen == ShellScreen::MainMenu)
}

fn write_selected_profile_name_into_authored_text(mut text: Mut<Text>, display_name: &str) {
    if text.0 != display_name {
        text.0.clear();
        text.0.push_str(display_name);
    }
}
