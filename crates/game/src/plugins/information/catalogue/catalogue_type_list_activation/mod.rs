use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::{
    world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueCategory,
    AssetId,
};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::animal_adoption_contracts::BeginAnimalAdoptionPlacement;
use crate::plugins::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use crate::plugins::progression::catalogue_entry_availability::{catalogue_entry_is_available, catalogue_entry_can_be_researched};
use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    super::catalogue_types::{
        CatalogueRowEntry, PurchaseChoice, RememberedCatalogueEntry, TypeListFilter,
    },
    catalogue_type_list_policy_operations::choose_remembered_or_first_included_catalogue_entry,
    catalogue_world_availability_context::resolve_catalogue_world_session_mode_and_scenario_availability,
};
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiTypeListPolicy;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(in crate::plugins::information) fn choose_catalogue_entry_from_activated_type_list_row(
    mut commands: Commands,
    mut ui_node_activations: MessageReader<UiNodeActivated>,
    type_lists: Query<(), With<UiTypeListPolicy>>,
    catalogue_rows: Query<&CatalogueRowEntry>,
    parent_entities: Query<&ChildOf>,
    document_owners: Query<
        &crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner,
    >,
    selected_genders: Query<
        &crate::plugins::animal_lifecycle::animal_adoption_contracts::AnimalCatalogueGender,
    >,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    research_availability: Query<
        &crate::plugins::progression::research_types::ResearchProjectAvailability,
    >,
    unlocked_catalogue_definitions: Res<UnlockedCatalogueDefinitionSet>,
    selected_worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    mut purchase_choices: MessageWriter<PurchaseChoice>,
    mut adoption_placement_requests: MessageWriter<BeginAnimalAdoptionPlacement>,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let (world_session_mode, scenario_availability) =
        resolve_catalogue_world_session_mode_and_scenario_availability(&selected_worlds);

    for activation in ui_node_activations.read() {
        if activation.trigger != UiTrigger::Press || world_definitions.is_none() {
            continue;
        }
        let Some(catalogue_row) = std::iter::successors(Some(activation.node), |entity| {
            parent_entities.get(*entity).ok().map(ChildOf::parent)
        })
        .find_map(|entity| catalogue_rows.get(entity).ok()) else {
            continue;
        };

        let selected_entry_is_unavailable = world_definitions.is_some_and(|world_definitions| {
            world_definitions
                .catalogue_in_authored_purchase_order()
                .find(|(_, entry)| AssetId(entry.definition.0) == catalogue_row.definition)
                .is_none_or(|(catalogue_index, catalogue_entry)| {
                    !catalogue_entry_can_be_researched(
                        world_definitions,
                        catalogue_entry,
                        research_availability
                            .iter()
                            .map(|item| (item.item, item.available)),
                    ) && !catalogue_entry_is_available(
                        world_definitions,
                        catalogue_index,
                        catalogue_entry,
                        world_session_mode,
                        scenario_availability,
                        &unlocked_catalogue_definitions,
                    )
                })
        });
        if selected_entry_is_unavailable {
            continue;
        }

        if let Some(type_list) = std::iter::successors(Some(activation.node), |entity| {
            parent_entities.get(*entity).ok().map(ChildOf::parent)
        })
        .find(|entity| type_lists.contains(*entity))
        {
            commands
                .entity(type_list)
                .insert(RememberedCatalogueEntry(catalogue_row.definition));
        }

        let selected_entry_is_animal = world_definitions.is_some_and(|world_definitions| {
            world_definitions
                .catalogue()
                .find(|entry| AssetId(entry.definition.0) == catalogue_row.definition)
                .is_some_and(|entry| matches!(&entry.category, CatalogueCategory::Animals))
        });
        if selected_entry_is_animal {
            let sex = std::iter::successors(Some(activation.node), |entity| {
                parent_entities.get(*entity).ok().map(ChildOf::parent)
            })
            .find_map(|entity| {
                selected_genders
                    .get(entity)
                    .ok()
                    .or_else(|| {
                        document_owners
                            .get(entity)
                            .ok()
                            .and_then(|owner| selected_genders.get(owner.0).ok())
                    })
                    .map(|gender| gender.0)
            });
            adoption_placement_requests.write(BeginAnimalAdoptionPlacement {
                species: catalogue_row.definition,
                sex,
                offer_slot_index: None,
            });
        } else {
            purchase_choices.write(PurchaseChoice {
                definition: catalogue_row.definition,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use openzt2_game_data::AssetId;

    use super::*;
    use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
    use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;
    use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::{
        CatalogueCategory, CatalogueEntry, CatalogueFilterFlags,
    };
    use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;

    fn loaded_catalogue_app(definitions: &[AssetId]) -> App {
        let mut app = App::new();
        app.init_resource::<Assets<WorldDefinitionAsset>>()
            .init_resource::<WorldDefinitions>()
            .init_resource::<UnlockedCatalogueDefinitionSet>()
            .add_message::<UiNodeActivated>()
            .add_message::<PurchaseChoice>()
            .add_systems(Update, choose_catalogue_entry_from_activated_type_list);

        let document = WorldDefinitionDocument {
            catalogue: definitions
                .iter()
                .copied()
                .map(|definition| CatalogueEntry {
                    filter_values: Vec::new(),
                    id: definition,
                    definition,
                    kind: AssetId::default(),
                    kinds: Vec::new(),
                    category: CatalogueCategory::Facilities,
                    authored_purchase_sort_key: String::new(),
                    authored_purchase_sort_fallback_type_name: String::new(),
                    authored_type_registry_source_order: [0; 3],
                    filters: CatalogueFilterFlags::default(),
                    name_key: AssetId::default(),
                    icon: AssetId::default(),
                })
                .collect(),
            ..Default::default()
        };
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(document.clone()));
        app.world_mut()
            .resource_mut::<WorldDefinitions>()
            .index_test_document(handle, &document);
        app
    }

    #[test]
    fn same_frame_row_press_drains_all_events_without_delayed_fallback_choice() {
        let definition = AssetId([7; 16]);
        let mut app = loaded_catalogue_app(&[definition]);
        let type_list = app
            .world_mut()
            .spawn(UiTypeListPolicy {
                included_kinds: Vec::new(),
                excluded_kinds: Vec::new(),
                filters: Vec::new(),
            })
            .id();
        let row_one = app.world_mut().spawn(CatalogueRowEntry { definition }).id();
        let row_two = app.world_mut().spawn(CatalogueRowEntry { definition }).id();
        let row_one_button = app.world_mut().spawn(ChildOf(row_one)).id();
        let row_two_button = app.world_mut().spawn(ChildOf(row_two)).id();

        app.world_mut().write_message(UiNodeActivated {
            source: crate::plugins::input::input_types::ActionSource::System,
            node: type_list,
            trigger: UiTrigger::On,
        });
        app.update();
        let choices = app
            .world_mut()
            .resource_mut::<Messages<PurchaseChoice>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(choices, vec![PurchaseChoice { definition }]);

        // The source messages from the control remain in Messages; only the
        // output choice above is drained before the suppression case.
        app.world_mut().write_message(UiNodeActivated {
            source: crate::plugins::input::input_types::ActionSource::System,
            node: type_list,
            trigger: UiTrigger::On,
        });
        for node in [row_one_button, row_two_button] {
            app.world_mut().write_message(UiNodeActivated {
                source: crate::plugins::input::input_types::ActionSource::KeyboardMouse,
                node,
                trigger: UiTrigger::Press,
            });
        }
        app.update();
        assert!(app
            .world()
            .resource::<Messages<PurchaseChoice>>()
            .is_empty());

        // Removing the second row makes an unread second row press unable to
        // suppress the stale On event; a delayed choice would expose either
        // reader's incomplete consumption on this update.
        app.world_mut()
            .entity_mut(row_two)
            .remove::<CatalogueRowEntry>();
        app.update();
        assert!(app
            .world()
            .resource::<Messages<PurchaseChoice>>()
            .is_empty());
    }

    #[test]
    fn lists_retain_their_own_pressed_child_across_other_list_activation() {
        let first = AssetId([1; 16]);
        let second = AssetId([2; 16]);
        let mut app = loaded_catalogue_app(&[first, second]);
        app.add_message::<BeginAnimalAdoptionPlacement>()
            .add_systems(
                Update,
                choose_catalogue_entry_from_activated_type_list_row
                    .before(choose_catalogue_entry_from_activated_type_list),
            );
        let mut create_list = || {
            app.world_mut()
                .spawn(UiTypeListPolicy {
                    included_kinds: Vec::new(),
                    excluded_kinds: Vec::new(),
                    filters: Vec::new(),
                })
                .id()
        };
        let list_one = create_list();
        let list_two = create_list();
        let row = app
            .world_mut()
            .spawn((CatalogueRowEntry { definition: second }, ChildOf(list_one)))
            .id();
        for (node, trigger, expected) in [
            (row, UiTrigger::Press, second),
            (list_two, UiTrigger::On, first),
            (list_one, UiTrigger::On, second),
        ] {
            app.world_mut().write_message(UiNodeActivated {
                source: crate::plugins::input::input_types::ActionSource::KeyboardMouse,
                node,
                trigger,
            });
            app.update();
            let choices: Vec<_> = app
                .world_mut()
                .resource_mut::<Messages<PurchaseChoice>>()
                .drain()
                .collect();
            assert_eq!(
                choices,
                [PurchaseChoice {
                    definition: expected
                }]
            );
        }
        assert_eq!(
            app.world()
                .get::<RememberedCatalogueEntry>(list_one)
                .unwrap()
                .0,
            second
        );
        assert_eq!(
            app.world()
                .get::<RememberedCatalogueEntry>(list_two)
                .unwrap()
                .0,
            first
        );
    }
}

/// Selects a catalogue entry when an authored construction type list is
/// activated (`UI_ACTIVATE_ON`): retain that list's remembered valid entry,
/// otherwise use the first included and available entry in authored purchase
/// order. Same-update row presses suppress this fallback so explicit selection
/// wins. The native inherited toggle-set selection belongs to each list.
pub(in crate::plugins::information) fn choose_catalogue_entry_from_activated_type_list(
    mut commands: Commands,
    mut ui_node_activations: MessageReader<UiNodeActivated>,
    mut row_activations: MessageReader<UiNodeActivated>,
    type_lists: Query<(
        &UiTypeListPolicy,
        Option<&TypeListFilter>,
        Option<&RememberedCatalogueEntry>,
    )>,
    catalogue_rows: Query<&CatalogueRowEntry>,
    parent_entities: Query<&ChildOf>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    unlocked_catalogue_definitions: Res<UnlockedCatalogueDefinitionSet>,
    selected_worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    mut purchase_choices: MessageWriter<PurchaseChoice>,
) {
    // Consume the row stream completely before making the precedence decision.
    // Otherwise a short-circuiting reader could leave a later row event behind.
    let mut explicit_catalogue_row_press = false;
    for activation in row_activations.read() {
        if activation.trigger == UiTrigger::Press
            && std::iter::successors(Some(activation.node), |entity| {
                parent_entities.get(*entity).ok().map(ChildOf::parent)
            })
            .any(|entity| catalogue_rows.get(entity).is_ok())
        {
            explicit_catalogue_row_press = true;
        }
    }
    if explicit_catalogue_row_press {
        // The TypeList stream must also be consumed when row selection wins;
        // otherwise an On event can replay as a delayed fallback choice.
        for _ in ui_node_activations.read() {}
        return;
    }

    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        for _ in ui_node_activations.read() {}
        return;
    };
    let (world_session_mode, scenario_availability) =
        resolve_catalogue_world_session_mode_and_scenario_availability(&selected_worlds);

    for activation in ui_node_activations.read() {
        if activation.trigger != UiTrigger::On {
            continue;
        }
        let Ok((type_list_policy, type_list_filter, remembered)) = type_lists.get(activation.node)
        else {
            continue;
        };
        let Some((_, catalogue_entry)) = choose_remembered_or_first_included_catalogue_entry(
            type_list_policy,
            type_list_filter.copied().unwrap_or_default(),
            remembered.map(|entry| entry.0),
            world_definitions,
            world_session_mode,
            scenario_availability,
            &unlocked_catalogue_definitions,
        ) else {
            continue;
        };
        commands
            .entity(activation.node)
            .insert(RememberedCatalogueEntry(AssetId(
                catalogue_entry.definition.0,
            )));
        purchase_choices.write(PurchaseChoice {
            definition: AssetId(catalogue_entry.definition.0),
        });
    }
}
