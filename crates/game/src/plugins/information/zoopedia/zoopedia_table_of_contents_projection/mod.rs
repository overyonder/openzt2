use bevy::prelude::*;
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_game_data::AssetId;
use std::collections::HashSet;

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::ui::authored_button_runtime_policy::UiButtonPolicy;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use crate::plugins::ui::authored_tree_expansion_and_row_indentation::UiAuthoredTreeExpansion;
use crate::plugins::ui::authored_tree_expansion_and_row_indentation::UiAuthoredTreeItem;
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;

use super::{
    zoopedia_hierarchy_operations::{
        collect_visible_zoopedia_entries, find_root_zoopedia_entry, resolve_zoopedia_entry_subject,
        zoopedia_entry_has_children,
    },
    zoopedia_navigation_types::ZoopediaPage,
    zoopedia_table_of_contents_types::{ZoopediaTableOfContentsRow, ZoopediaTableOfContentsState},
};

pub(in crate::plugins::information) fn project_zoopedia_table_of_contents_for_hydrated_pages(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    lists: Query<(Entity, &UiListPolicy, &UiDocumentOwner), Without<ZoopediaTableOfContentsState>>,
    pages: Query<(), With<ZoopediaPage>>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let catalog = definitions;
    let Some(root) = find_root_zoopedia_entry(catalog) else {
        return;
    };

    for (list, policy, owner) in &lists {
        if policy.source != UiWidgetLiveCollectionSource::ZoopediaTableOfContents
            || pages.get(owner.0).is_err()
        {
            continue;
        }
        commands.entity(list).insert(ZoopediaTableOfContentsState {
            expanded_subjects: HashSet::from([resolve_zoopedia_entry_subject(root)]),
            projected_row_count: 0,
        });
    }
}

/// Copies only the authored expander interaction into the list presentation
/// state. Entry relationships themselves continue to come from the loaded
/// world definitions.
pub(in crate::plugins::information) fn apply_authored_zoopedia_table_of_contents_expansion_changes(
    changed_trees: Query<
        (Entity, &UiAuthoredTreeExpansion),
        (With<UiAuthoredTreeItem>, Changed<UiAuthoredTreeExpansion>),
    >,
    parents: Query<&ChildOf>,
    rows: Query<(&UiListRow, &ZoopediaTableOfContentsRow)>,
    mut lists: Query<&mut ZoopediaTableOfContentsState>,
) {
    for (tree_entity, expanded) in &changed_trees {
        let Some((row, presentation)) = std::iter::successors(Some(tree_entity), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .find_map(|entity| rows.get(entity).ok()) else {
            continue;
        };
        let Ok(mut state) = lists.get_mut(row.list) else {
            continue;
        };
        if expanded.is_expanded() {
            state.expanded_subjects.insert(presentation.subject);
        } else {
            state.expanded_subjects.remove(&presentation.subject);
        }
    }
}

pub(in crate::plugins::information) fn request_zoopedia_table_of_contents_row_counts(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut lists: Query<(Entity, &mut ZoopediaTableOfContentsState)>,
    mut row_counts: MessageWriter<SetUiListRowCount>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let catalog = definitions;
    let Some(root) = find_root_zoopedia_entry(catalog) else {
        return;
    };
    for (list, mut state) in &mut lists {
        let count = u16::try_from(
            collect_visible_zoopedia_entries(catalog, root, &state.expanded_subjects).len(),
        )
        .unwrap_or(u16::MAX);
        if state.projected_row_count != count {
            state.projected_row_count = count;
            row_counts.write(SetUiListRowCount { list, count });
        }
    }
}

/// Hydrates the authored TOC row fragment with stable references into the
/// loaded Zoopedia hierarchy.
pub(in crate::plugins::information) fn project_zoopedia_table_of_contents_rows(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    rows: Query<(Entity, &UiListRow, Option<&ZoopediaTableOfContentsRow>)>,
    lists: Query<(&UiDocumentOwner, &ZoopediaTableOfContentsState)>,
    parents: Query<&ChildOf>,
    mut buttons: Query<(
        Entity,
        &UiButtonPolicy,
        &mut UiInteractionEnabled,
        Option<&mut UiSelected>,
        Option<&ZoopediaTableOfContentsRow>,
    )>,
    mut trees: Query<(
        Entity,
        &mut UiAuthoredTreeItem,
        &mut UiAuthoredTreeExpansion,
    )>,
    mut texts: Query<(Entity, &mut Text)>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    let catalog = definitions;
    let Some(root) = find_root_zoopedia_entry(catalog) else {
        return;
    };
    for (row_entity, row, current) in &rows {
        let Ok((owner, state)) = lists.get(row.list) else {
            continue;
        };
        let visible = collect_visible_zoopedia_entries(catalog, root, &state.expanded_subjects);
        let Some((entry, depth)) = visible.get(usize::from(row.index)).copied() else {
            continue;
        };
        let presentation = ZoopediaTableOfContentsRow {
            subject: resolve_zoopedia_entry_subject(entry),
            zoopedia_page_entity: owner.0,
        };
        let expandable = zoopedia_entry_has_children(catalog, entry);
        let identity_changed = current != Some(&presentation);
        if identity_changed {
            commands.entity(row_entity).insert(presentation);
        }
        let expanded_value = expandable && state.expanded_subjects.contains(&presentation.subject);
        for (entity, mut tree, mut expanded) in &mut trees {
            if !has_ancestor(entity, row_entity, &parents) {
                continue;
            }
            if tree.authored_item() != presentation.subject {
                tree.set_authored_item(presentation.subject);
            }
            if tree.hierarchy_depth() != depth {
                tree.set_hierarchy_depth(depth);
            }
            if identity_changed && expanded.is_expanded() != expanded_value {
                expanded.set_expanded(expanded_value);
            }
        }
        for (entity, policy, mut enabled, selected, current_action) in &mut buttons {
            if !has_ancestor(entity, row_entity, &parents) {
                continue;
            }
            if policy.selectable {
                enabled.0 = expandable;
                if current_action.is_some() {
                    commands
                        .entity(entity)
                        .remove::<ZoopediaTableOfContentsRow>();
                }
                if let Some(mut selected) = selected {
                    selected.0 = expanded_value;
                }
            } else {
                enabled.0 = true;
                if current_action != Some(&presentation) {
                    commands.entity(entity).insert(presentation);
                }
            }
        }
        let Some(label) = localization.find_plain_localized_text(AssetId(entry.title_key.0)) else {
            continue;
        };
        for (entity, mut text) in &mut texts {
            if has_ancestor(entity, row_entity, &parents) && text.0 != label {
                text.0.clear();
                text.0.push_str(label);
            }
        }
    }
}

fn has_ancestor(entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    std::iter::successors(Some(entity), |entity| {
        parents.get(*entity).ok().map(ChildOf::parent)
    })
    .skip(1)
    .any(|entity| entity == ancestor)
}
