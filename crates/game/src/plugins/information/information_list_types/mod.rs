use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::InformationListCategory;

/// View-local state for an entity multilist. Rows refer directly to canonical
/// world entities through `InformationEntitySource`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InformationEntityListPanel {
    pub(super) category: InformationListCategory,
    pub(super) sort: InformationEntityListSort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InformationEntityListSort {
    SourceOrder,
    Type { descending: bool },
    Name { descending: bool },
    Need { descending: bool },
    AnimalHappiness { descending: bool },
    AnimalPregnancy { descending: bool },
    GuestFavouriteAnimal,
    MonthsOpen { descending: bool },
    Profit { descending: bool },
    AverageProfit { descending: bool },
    CurrentCapacity { descending: bool },
    TotalCapacity { descending: bool },
}

/// One bounded, frame-local row assembled from canonical ECS components.
/// It carries only the sortable values required by the authored information
/// list and never becomes another owner of game state.
#[derive(Clone, Copy)]
pub(super) struct InformationListRowProjectionCandidate<'a> {
    pub(super) entity: Entity,
    pub(super) definition: [u8; 16],
    pub(super) name: &'a str,
    pub(super) welfare: u16,
    pub(super) pregnant: bool,
    pub(super) satisfaction: u16,
    pub(super) favourite_animal: Option<[u8; 16]>,
    pub(super) months_open: u32,
    pub(super) profit_cents: i64,
    pub(super) average_profit_cents: i64,
    pub(super) current_capacity: u16,
    pub(super) total_capacity: u16,
}
