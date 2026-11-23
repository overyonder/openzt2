use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use std::collections::HashSet;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::ui::authored_button_runtime_policy::UiButtonPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::RequestedUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_ui::authored_tree_expansion_and_row_indentation::UiAuthoredTreeExpansion;
use openzt2_ui::authored_tree_expansion_and_row_indentation::UiAuthoredTreeItem;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;

use super::{
    zoopedia_hierarchy_operations::{
        collect_visible_zoopedia_entries, resolve_zoopedia_entry_subject,
        zoopedia_entry_has_children,
    },
    zoopedia_navigation_types::{ZoopediaPage, ZoopediaRootEntry},
    zoopedia_table_of_contents_types::{
        VisibleZoopediaEntries, VisibleZoopediaEntry, ZoopediaTableOfContentsRow,
        ZoopediaTableOfContentsState,
    },
};

pub(crate) fn project_zoopedia_table_of_contents_for_hydrated_pages(
    lists: Query<(Entity, &UiListPolicy, &UiDocumentOwner), Without<ZoopediaTableOfContentsState>>,
    pages: Query<(), With<ZoopediaPage>>,
    mut commands: Commands,
) {
    for (list, policy, owner) in &lists {
        if policy.source != UiWidgetLiveCollectionSource::ZoopediaTableOfContents
            || pages.get(owner.0).is_err()
        {
            continue;
        }
        commands.entity(list).insert((
            ZoopediaTableOfContentsState {
                expanded_subjects: HashSet::new(),
            },
            VisibleZoopediaEntries::default(),
        ));
    }
}

pub(crate) fn apply_authored_zoopedia_table_of_contents_expansion_changes(
    mut activations: MessageReader<UiNodeActivated>,
    expanders: Query<&UiInteractionEnabled, With<UiAuthoredTreeItem>>,
    parents: Query<&ChildOf>,
    rows: Query<(&ChildOf, &ZoopediaTableOfContentsRow), With<UiListRow>>,
    mut lists: Query<&mut ZoopediaTableOfContentsState>,
) {
    for activation in activations.read() {
        if activation.trigger != UiTrigger::Press
            || !expanders
                .get(activation.node)
                .is_ok_and(|enabled| enabled.0)
        {
            continue;
        }
        let Some((parent, row)) = std::iter::successors(Some(activation.node), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .find_map(|entity| rows.get(entity).ok()) else {
            continue;
        };
        let Ok(mut state) = lists.get_mut(parent.parent()) else {
            continue;
        };
        if !state.expanded_subjects.remove(&row.subject) {
            state.expanded_subjects.insert(row.subject);
        }
    }
}

pub(crate) fn reconcile_visible_zoopedia_entries(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    roots: Query<Ref<ZoopediaRootEntry>>,
    mut lists: Query<(
        &UiDocumentOwner,
        &mut ZoopediaTableOfContentsState,
        &mut VisibleZoopediaEntries,
        &mut RequestedUiListRowCount,
    )>,
) {
    let hierarchy_changed = definitions.is_changed() || active_definitions.is_changed();
    let Some(catalog) = active_definitions.get(&definitions) else {
        return;
    };
    for (owner, mut state, mut entries, mut count) in &mut lists {
        let Ok(root_entry) = roots.get(owner.0) else {
            entries.set_if_neq(VisibleZoopediaEntries::default());
            count.set_if_neq(RequestedUiListRowCount(Some(0)));
            continue;
        };
        if !hierarchy_changed
            && !root_entry.is_changed()
            && !state.is_changed()
            && count.0.is_some()
        {
            continue;
        }
        let Some(root) = catalog.find_zoopedia_entry(root_entry.0) else {
            entries.set_if_neq(VisibleZoopediaEntries::default());
            count.set_if_neq(RequestedUiListRowCount(Some(0)));
            continue;
        };
        if root_entry.is_changed() || state.is_added() || count.0.is_none() {
            state
                .expanded_subjects
                .insert(resolve_zoopedia_entry_subject(root));
        }
        let visible = collect_visible_zoopedia_entries(catalog, root, &state.expanded_subjects)
            .into_iter()
            .map(|(entry, depth)| VisibleZoopediaEntry {
                subject: resolve_zoopedia_entry_subject(entry),
                title_key: AssetId(entry.title_key.0),
                depth,
                expandable: zoopedia_entry_has_children(catalog, entry),
            })
            .collect::<Vec<_>>();
        let row_count = u16::try_from(visible.len()).unwrap_or(u16::MAX);
        count.set_if_neq(RequestedUiListRowCount(Some(row_count)));
        entries.set_if_neq(VisibleZoopediaEntries(visible));
    }
}

/// Hydrates the authored TOC row fragment with stable references into the
/// loaded Zoopedia hierarchy.
pub(crate) fn project_zoopedia_table_of_contents_rows(
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut rows: Query<(
        Entity,
        Ref<UiListRow>,
        Ref<ChildOf>,
        Option<&mut ZoopediaTableOfContentsRow>,
    )>,
    lists: Query<(
        &UiDocumentOwner,
        Ref<ZoopediaTableOfContentsState>,
        Ref<VisibleZoopediaEntries>,
    )>,
    children: Query<&Children>,
    mut buttons: Query<
        (
            Entity,
            &UiButtonPolicy,
            &mut UiInteractionEnabled,
            Option<&mut UiSelected>,
            Option<&mut ZoopediaTableOfContentsRow>,
        ),
        Without<UiListRow>,
    >,
    mut trees: Query<(
        Entity,
        &mut UiAuthoredTreeItem,
        &mut UiAuthoredTreeExpansion,
    )>,
    mut texts: Query<(Entity, &mut Text)>,
    mut commands: Commands,
) {
    let locale_changed = active_localization.is_changed() || localizations.is_changed();
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    for (row_entity, row, parent, current) in &mut rows {
        let Ok((owner, state, visible)) = lists.get(parent.parent()) else {
            continue;
        };
        if current.is_some()
            && !row.is_changed()
            && !parent.is_changed()
            && !state.is_changed()
            && !visible.is_changed()
            && !locale_changed
        {
            continue;
        }
        let Some(entry) = visible.0.get(usize::from(row.index)) else {
            continue;
        };
        let presentation = ZoopediaTableOfContentsRow {
            subject: entry.subject,
            zoopedia_page_entity: owner.0,
        };
        if let Some(mut current) = current {
            current.set_if_neq(presentation);
        } else {
            commands.entity(row_entity).insert(presentation);
        }
        let expanded_value = entry.expandable && state.expanded_subjects.contains(&entry.subject);
        let label = localization.find_plain_localized_text(entry.title_key);
        for entity in children.iter_descendants::<Children>(row_entity) {
            if let Ok((_, mut tree, mut expanded)) = trees.get_mut(entity) {
                if tree.authored_item() != entry.subject {
                    tree.set_authored_item(entry.subject);
                }
                if tree.hierarchy_depth() != entry.depth {
                    tree.set_hierarchy_depth(entry.depth);
                }
                if expanded.is_expanded() != expanded_value {
                    expanded.set_expanded(expanded_value);
                }
            }
            if let Ok((entity, policy, mut enabled, selected, current_action)) =
                buttons.get_mut(entity)
            {
                enabled.set_if_neq(UiInteractionEnabled(!policy.selectable || entry.expandable));
                if policy.selectable {
                    if current_action.is_some() {
                        commands
                            .entity(entity)
                            .remove::<ZoopediaTableOfContentsRow>();
                    }
                    if let Some(mut selected) = selected {
                        selected.set_if_neq(UiSelected(expanded_value));
                    }
                } else if let Some(mut current_action) = current_action {
                    current_action.set_if_neq(presentation);
                } else {
                    commands.entity(entity).insert(presentation);
                }
            }
            if let Some(label) = label {
                if let Ok((_, mut text)) = texts.get_mut(entity) {
                    if text.0 != label {
                        text.0.clear();
                        text.0.push_str(label);
                    }
                }
            }
        }
    }
}
