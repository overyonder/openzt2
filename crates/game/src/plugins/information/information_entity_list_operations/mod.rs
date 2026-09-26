use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::{
    InformationListCategory, InformationListCriterion, InformationSortField, UiInformationAction,
};
use openzt2_game_data::AssetId;

use crate::plugins::ui::authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId};

use super::information_list_types::{InformationEntityListPanel, InformationEntityListSort};

pub(super) fn apply_authored_information_entity_list_action(
    action: &UiInformationAction,
    document_owner_entity: Entity,
    ui_nodes: &Query<(Entity, &UiDocumentOwner, &UiNodeId)>,
    entity_list_panels: &mut Query<(Entity, &mut InformationEntityListPanel)>,
    commands: &mut Commands,
) {
    match action {
        UiInformationAction::PopulateEntityList { list, category } => {
            let Some(target_list_entity) =
                find_authored_information_list_entity(document_owner_entity, *list, ui_nodes)
            else {
                return;
            };
            update_or_create_information_entity_list_panel(
                target_list_entity,
                entity_list_panels,
                commands,
                |entity_list_panel| entity_list_panel.category = *category,
            );
        }
        UiInformationAction::SortEntityList { list, criterion } => {
            let entity_list_sort = match criterion {
                InformationListCriterion::Type => {
                    InformationEntityListSort::Type { descending: false }
                }
                InformationListCriterion::Name => {
                    InformationEntityListSort::Name { descending: false }
                }
            };
            set_information_entity_list_sort(
                document_owner_entity,
                *list,
                entity_list_sort,
                ui_nodes,
                entity_list_panels,
                commands,
            );
        }
        UiInformationAction::SortEntityListDirected {
            list,
            field,
            descending,
        } => {
            let entity_list_sort = match field {
                InformationSortField::Name => InformationEntityListSort::Name {
                    descending: *descending,
                },
                InformationSortField::Type => InformationEntityListSort::Type {
                    descending: *descending,
                },
                InformationSortField::MonthsOpen => InformationEntityListSort::MonthsOpen {
                    descending: *descending,
                },
                InformationSortField::Profit => InformationEntityListSort::Profit {
                    descending: *descending,
                },
                InformationSortField::AverageProfit => InformationEntityListSort::AverageProfit {
                    descending: *descending,
                },
                InformationSortField::CurrentCapacity => {
                    InformationEntityListSort::CurrentCapacity {
                        descending: *descending,
                    }
                }
                InformationSortField::TotalCapacity => InformationEntityListSort::TotalCapacity {
                    descending: *descending,
                },
            };
            set_information_entity_list_sort(
                document_owner_entity,
                *list,
                entity_list_sort,
                ui_nodes,
                entity_list_panels,
                commands,
            );
        }
        UiInformationAction::SortEntityListByNeed { list, descending } => {
            set_information_entity_list_sort(
                document_owner_entity,
                *list,
                InformationEntityListSort::Need {
                    descending: *descending,
                },
                ui_nodes,
                entity_list_panels,
                commands,
            );
        }
        UiInformationAction::SortAnimalsByHappiness { list, descending } => {
            set_information_entity_list_sort(
                document_owner_entity,
                *list,
                InformationEntityListSort::AnimalHappiness {
                    descending: *descending,
                },
                ui_nodes,
                entity_list_panels,
                commands,
            );
        }
        UiInformationAction::SortAnimalsByPregnancy { list, descending } => {
            set_information_entity_list_sort(
                document_owner_entity,
                *list,
                InformationEntityListSort::AnimalPregnancy {
                    descending: *descending,
                },
                ui_nodes,
                entity_list_panels,
                commands,
            );
        }
        UiInformationAction::SortGuestsByFavouriteAnimal { list } => {
            set_information_entity_list_sort(
                document_owner_entity,
                *list,
                InformationEntityListSort::GuestFavouriteAnimal,
                ui_nodes,
                entity_list_panels,
                commands,
            );
        }
        _ => unreachable!("non-list action sent to the information entity-list action owner"),
    }
}

fn set_information_entity_list_sort(
    document_owner_entity: Entity,
    list: AssetId,
    entity_list_sort: InformationEntityListSort,
    ui_nodes: &Query<(Entity, &UiDocumentOwner, &UiNodeId)>,
    entity_list_panels: &mut Query<(Entity, &mut InformationEntityListPanel)>,
    commands: &mut Commands,
) {
    let Some(target_list_entity) =
        find_authored_information_list_entity(document_owner_entity, list, ui_nodes)
    else {
        return;
    };
    update_or_create_information_entity_list_panel(
        target_list_entity,
        entity_list_panels,
        commands,
        |entity_list_panel| entity_list_panel.sort = entity_list_sort,
    );
}

fn find_authored_information_list_entity(
    document_owner_entity: Entity,
    list: AssetId,
    ui_nodes: &Query<(Entity, &UiDocumentOwner, &UiNodeId)>,
) -> Option<Entity> {
    ui_nodes
        .iter()
        .find(|(_, owner, node_id)| owner.0 == document_owner_entity && node_id.id == list)
        .map(|(entity, _, _)| entity)
}

fn update_or_create_information_entity_list_panel(
    target_list_entity: Entity,
    entity_list_panels: &mut Query<(Entity, &mut InformationEntityListPanel)>,
    commands: &mut Commands,
    update_entity_list_panel: impl Fn(&mut InformationEntityListPanel),
) {
    if let Ok((_, mut entity_list_panel)) = entity_list_panels.get_mut(target_list_entity) {
        update_entity_list_panel(&mut entity_list_panel);
    } else {
        let mut entity_list_panel = InformationEntityListPanel {
            category: InformationListCategory::Animals,
            sort: InformationEntityListSort::SourceOrder,
        };
        update_entity_list_panel(&mut entity_list_panel);
        commands
            .entity(target_list_entity)
            .insert(entity_list_panel);
    }
}
