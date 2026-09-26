//! Native purchase-menu field selection over the canonical catalogue.
use openzt2_game_data::ui_document::action::UiTrigger;

use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiTypeListPolicy;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_node_projection_components::UiNodeId;
use crate::plugins::ui::projection::authored_document_projection::flow_authored_row;
use crate::plugins::ui::projection::authored_document_projection::project_document;
use crate::plugins::ui::ui_document_lifecycle_contracts::ShowUiDocument;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    super::catalogue_types::TypeListFilter,
    catalogue_type_list_policy_operations::catalogue_entry_is_included_by_authored_type_list,
    catalogue_world_availability_context::resolve_catalogue_world_session_mode_and_scenario_availability,
};

/// A native-created row assigns one field/value to the active authored list.
#[derive(Component)]
pub(in crate::plugins::information) struct CatalogueFilterChoice {
    type_list: Entity,
    drop_list: Entity,
    field_value: Option<(AssetId, AssetId)>,
    label: AssetId,
}

/// Populate only when opening a menu; settled frames do not scan the catalogue.
pub(in crate::plugins::information) fn populate_catalogue_filter_menu(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    nodes: Query<(Entity, &UiNodeId, &UiDocumentOwner)>,
    drop_lists: Query<(Entity, &UiDocumentOwner, &UiListPolicy)>,
    type_lists: Query<(
        Entity,
        &UiDocumentOwner,
        &UiTypeListPolicy,
        &InheritedVisibility,
    )>,
    parents: Query<&ChildOf>,
    existing_rows: Query<(Entity, &UiListRow)>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    localization_index: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
) {
    for activation in activations
        .read()
        .filter(|event| event.trigger == UiTrigger::Press)
    {
        let Ok((_, activated_id, activated_owner)) = nodes.get(activation.node) else {
            continue;
        };
        for (drop_entity, owner, policy) in &drop_lists {
            if owner != activated_owner
                || policy.opener_node != activated_id.id
                || policy.drop_list_display_node == AssetId::default()
            {
                continue;
            }
            let Some((menu, _, _)) = nodes.iter().find(|(_, id, candidate)| {
                *candidate == owner && id.id == policy.drop_list_display_node
            }) else {
                continue;
            };
            let ancestry = std::iter::successors(Some(drop_entity), |entity| {
                parents.get(*entity).ok().map(ChildOf::parent)
            })
            .collect::<Vec<_>>();
            let Some((type_list, _, type_policy, _)) = type_lists
                .iter()
                .filter(|(_, candidate, policy, visible)| {
                    *candidate == owner && visible.get() && !policy.filters.is_empty()
                })
                .min_by_key(|(entity, _, _, _)| {
                    std::iter::successors(Some(*entity), |entity| {
                        parents.get(*entity).ok().map(ChildOf::parent)
                    })
                    .filter_map(|ancestor| {
                        ancestry.iter().position(|candidate| *candidate == ancestor)
                    })
                    .min()
                    .unwrap_or(usize::MAX)
                })
            else {
                continue;
            };
            let Some(document) = roots
                .get(owner.0)
                .ok()
                .and_then(|root| documents.get(&root.document))
            else {
                continue;
            };
            let Some(row_handle) = document.nested_ui_document_handle(AssetId::from_virtual_path(
                "ui/fragment/ui/layout/filterlistitem.xml",
            )) else {
                continue;
            };
            let Some(divider_handle) = document.nested_ui_document_handle(
                AssetId::from_virtual_path("ui/fragment/ui/layout/verticaldivide.xml"),
            ) else {
                continue;
            };
            let (Some(row_document), Some(divider_document), Some(catalogue), Some(localization)) = (
                documents.get(row_handle),
                documents.get(divider_handle),
                active_definitions.get(&definitions),
                localization_index.borrow_loaded_localization_view(&localizations),
            ) else {
                continue;
            };
            let (mode, scenario) =
                resolve_catalogue_world_session_mode_and_scenario_availability(&worlds);
            let mut fields = BTreeMap::<&str, BTreeMap<&str, &str>>::new();
            for (index, entry) in catalogue.catalogue_in_authored_purchase_order() {
                if !catalogue_entry_is_included_by_authored_type_list(
                    type_policy,
                    TypeListFilter::default(),
                    index,
                    entry,
                    catalogue,
                    mode,
                    scenario,
                    &unlocks,
                ) {
                    continue;
                }
                for value in &entry.filter_values {
                    let label = AssetId::from_key(
                        &format!("filtertext:{}_{}", value.field, value.value).to_ascii_lowercase(),
                    );
                    if type_policy
                        .filters
                        .contains(&AssetId::from_key(&value.field))
                        && localization.find_plain_localized_text(label).is_some()
                    {
                        fields
                            .entry(&value.field)
                            .or_default()
                            .insert(&value.value, &value.value);
                    }
                }
            }
            for (entity, row) in &existing_rows {
                if row.list == drop_entity {
                    commands.entity(entity).despawn();
                }
            }
            let all = project_document(
                &mut commands,
                &ShowUiDocument {
                    document: row_handle.clone(),
                    owner: menu,
                },
                row_document,
                localization,
                None,
                false,
            );
            commands.entity(all).insert((
                UiListRow {
                    list: drop_entity,
                    index: 0,
                },
                CatalogueFilterChoice {
                    type_list,
                    drop_list: drop_entity,
                    field_value: None,
                    label: AssetId::from_key("filtertext:all_all"),
                },
            ));
            let mut index = 1_u16;
            for (field, values) in fields {
                let divider = project_document(
                    &mut commands,
                    &ShowUiDocument {
                        document: divider_handle.clone(),
                        owner: menu,
                    },
                    divider_document,
                    localization,
                    None,
                    false,
                );
                commands.entity(divider).insert(UiListRow {
                    list: drop_entity,
                    index,
                });
                index = index.saturating_add(1);
                for value in values.keys() {
                    let row = project_document(
                        &mut commands,
                        &ShowUiDocument {
                            document: row_handle.clone(),
                            owner: menu,
                        },
                        row_document,
                        localization,
                        None,
                        false,
                    );
                    commands.entity(row).insert((
                        UiListRow {
                            list: drop_entity,
                            index,
                        },
                        CatalogueFilterChoice {
                            type_list,
                            drop_list: drop_entity,
                            field_value: Some((
                                AssetId::from_key(field),
                                AssetId::from_key(&value.to_ascii_lowercase()),
                            )),
                            label: AssetId::from_key(
                                &format!("filtertext:{field}_{value}").to_ascii_lowercase(),
                            ),
                        },
                    ));
                    index = index.saturating_add(1);
                }
            }
        }
    }
}

/// Keep the authored row's font/style, replacing only its native-supplied text.
pub(in crate::plugins::information) fn label_catalogue_filter_rows(
    choices: Query<(Entity, &CatalogueFilterChoice), Added<CatalogueFilterChoice>>,
    rows: Query<&UiListRow>,
    mut nodes: Query<(&UiDocumentOwner, &mut Node, Option<&mut Text>)>,
    localization_index: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
) {
    let Some(localization) = localization_index.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    for (root, choice) in &choices {
        if rows.get(root).is_err() {
            continue;
        }
        for (owner, mut node, text) in &mut nodes {
            if owner.0 != root {
                continue;
            }
            if let Some(mut text) = text {
                if let Some(label) = localization.find_plain_localized_text(choice.label) {
                    text.0 = label.to_owned();
                }
                flow_authored_row(&mut node, Some(FlexDirection::Column), 0, 0);
            }
        }
    }
}

pub(in crate::plugins::information) fn select_catalogue_filter_row(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    choices: Query<&CatalogueFilterChoice>,
    parents: Query<&ChildOf>,
    filters: Query<&TypeListFilter>,
    drop_lists: Query<(&UiDocumentOwner, &UiListPolicy)>,
    nodes: Query<(Entity, &UiNodeId, &UiDocumentOwner)>,
    mut visibility: Query<&mut Visibility>,
    named_nodes: Query<(Entity, &Name)>,
    mut texts: Query<(Entity, &mut Text)>,
    localization_index: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
) {
    for activation in activations
        .read()
        .filter(|event| event.trigger == UiTrigger::Press)
    {
        let Some(choice) = std::iter::successors(Some(activation.node), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .find_map(|entity| choices.get(entity).ok()) else {
            continue;
        };
        let mut filter = filters.get(choice.type_list).copied().unwrap_or_default();
        filter.field_value = choice.field_value;
        commands.entity(choice.type_list).insert(filter);
        if let Some(localization) =
            localization_index.borrow_loaded_localization_view(&localizations)
        {
            if let Some(label) = localization.find_plain_localized_text(choice.label) {
                for (display, name) in &named_nodes {
                    if !name.as_str().eq_ignore_ascii_case("display")
                        || !std::iter::successors(Some(display), |entity| {
                            parents.get(*entity).ok().map(ChildOf::parent)
                        })
                        .any(|entity| entity == choice.drop_list)
                    {
                        continue;
                    }
                    for (entity, mut text) in &mut texts {
                        if std::iter::successors(Some(entity), |entity| {
                            parents.get(*entity).ok().map(ChildOf::parent)
                        })
                        .any(|entity| entity == display)
                        {
                            text.0 = label.to_owned();
                        }
                    }
                }
            }
        }
        if let Ok((owner, policy)) = drop_lists.get(choice.drop_list) {
            for (entity, id, candidate) in &nodes {
                if candidate == owner && id.id == policy.drop_list_display_node {
                    if let Ok(mut visible) = visibility.get_mut(entity) {
                        *visible = Visibility::Hidden;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::input::input_types::ActionSource;

    #[test]
    fn catalogue_filter_row_selects_field_without_discarding_research_or_kind() {
        let mut app = App::new();
        app.add_message::<UiNodeActivated>()
            .init_resource::<LocalizationPrecedenceIndex>()
            .init_resource::<Assets<LocalizationAsset>>()
            .add_systems(Update, select_catalogue_filter_row);
        let kind = AssetId::from_key("building");
        let field_value = Some((
            AssetId::from_key("s_objecttype"),
            AssetId::from_key("carts"),
        ));
        let list = app
            .world_mut()
            .spawn(TypeListFilter {
                unlocked_only: true,
                kind: Some(kind),
                field_value: None,
            })
            .id();
        let owner = UiDocumentOwner(app.world_mut().spawn_empty().id());
        let menu_id = AssetId::from_key("filterlist");
        let menu = app
            .world_mut()
            .spawn((
                owner,
                UiNodeId {
                    index: 0,
                    id: menu_id,
                },
                Visibility::Inherited,
            ))
            .id();
        let drop_list = app
            .world_mut()
            .spawn((
                owner,
                UiListPolicy {
                    row_document: None,
                    opener_node: AssetId::default(),
                    drop_list_display_node: menu_id,
                    source: Default::default(),
                },
            ))
            .id();
        let row = app
            .world_mut()
            .spawn(CatalogueFilterChoice {
                type_list: list,
                drop_list,
                field_value,
                label: AssetId::from_key("filtertext:s_objecttype_carts"),
            })
            .id();
        let hit = app.world_mut().spawn(ChildOf(row)).id();
        app.world_mut().write_message(UiNodeActivated {
            source: ActionSource::System,
            node: hit,
            trigger: UiTrigger::Press,
        });
        app.update();
        assert_eq!(
            app.world().get::<TypeListFilter>(list),
            Some(&TypeListFilter {
                unlocked_only: true,
                kind: Some(kind),
                field_value
            })
        );
        assert_eq!(
            app.world().get::<Visibility>(menu),
            Some(&Visibility::Hidden)
        );
    }
}
