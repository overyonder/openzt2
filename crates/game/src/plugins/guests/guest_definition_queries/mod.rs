use super::guest_simulation_types::GuestDessert;
use super::guest_simulation_types::GuestEnergy;
use super::guest_simulation_types::GuestGift;
use super::guest_simulation_types::GuestHunger;
use super::guest_simulation_types::GuestRestroom;
use super::guest_simulation_types::GuestSocial;
use super::guest_simulation_types::GuestThirst;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityServiceKind;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestDefinition;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestNeedDefinition;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestNeedKind;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestReactionDefinition;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestVisitPurpose;
use openzt2_game_data::world_definitions::world_objects::WorldObjectAffordanceFlags;
use openzt2_game_data::AssetId;

pub(super) fn find_guest_definition<'a>(
    definitions: WorldDefinitionsView<'a>,
    id: AssetId,
) -> Option<(WorldDefinitionsView<'a>, &'a GuestDefinition)> {
    definitions.find_guest(id).map(|row| (definitions, row))
}

pub(super) fn guest_need_rows<'a>(
    definition: &'a GuestDefinition,
) -> Option<[&'a GuestNeedDefinition; 7]> {
    let needs = &definition.needs;
    Some([
        needs
            .iter()
            .find(|need| matches!(&need.kind, GuestNeedKind::Hunger))?,
        needs
            .iter()
            .find(|need| matches!(&need.kind, GuestNeedKind::Thirst))?,
        needs
            .iter()
            .find(|need| matches!(&need.kind, GuestNeedKind::Dessert))?,
        needs
            .iter()
            .find(|need| matches!(&need.kind, GuestNeedKind::Gift))?,
        needs
            .iter()
            .find(|need| matches!(&need.kind, GuestNeedKind::Energy))?,
        needs
            .iter()
            .find(|need| matches!(&need.kind, GuestNeedKind::Restroom))?,
        needs
            .iter()
            .find(|need| matches!(&need.kind, GuestNeedKind::Social))?,
    ])
}

pub(super) fn reconsider_threshold(needs: [&GuestNeedDefinition; 7], kind: &GuestNeedKind) -> u16 {
    let index = match kind {
        GuestNeedKind::Hunger => 0,
        GuestNeedKind::Thirst => 1,
        GuestNeedKind::Dessert => 2,
        GuestNeedKind::Gift => 3,
        GuestNeedKind::Energy => 4,
        GuestNeedKind::Restroom => 5,
        GuestNeedKind::Social => 6,
    };
    needs[index].reconsider_threshold
}

pub(super) fn find_reaction<'a>(
    definition: &'a GuestDefinition,
    kind: GuestMemoryKind,
) -> Option<&'a GuestReactionDefinition> {
    definition
        .reactions
        .iter()
        .find(|reaction| reaction.kind == kind)
}

pub(super) fn need_value(
    kind: &GuestNeedKind,
    hunger: &GuestHunger,
    thirst: &GuestThirst,
    dessert: &GuestDessert,
    gift: &GuestGift,
    energy: &GuestEnergy,
    restroom: &GuestRestroom,
    social: &GuestSocial,
) -> u16 {
    match kind {
        GuestNeedKind::Hunger => hunger.value,
        GuestNeedKind::Thirst => thirst.value,
        GuestNeedKind::Dessert => dessert.value,
        GuestNeedKind::Gift => gift.value,
        GuestNeedKind::Energy => energy.value,
        GuestNeedKind::Restroom => restroom.value,
        GuestNeedKind::Social => social.value,
    }
}

pub(super) fn supports_purpose(
    asset: WorldDefinitionsView<'_>,
    definition: AssetId,
    purpose: GuestVisitPurpose,
) -> bool {
    let object = asset.find_object(definition);
    let facility = asset
        .find_facility(definition)
        .or_else(|| asset.find_facility_by_object(definition));
    let affordances = object.map(|object| object.affordances);
    match purpose {
        GuestVisitPurpose::View => affordances
            .is_some_and(|affordances| affordances.contains_all(WorldObjectAffordanceFlags::VIEW)),
        GuestVisitPurpose::Food => {
            affordances.is_some_and(|affordances| {
                affordances.contains_all(WorldObjectAffordanceFlags::EAT)
            }) || facility
                .is_some_and(|facility| matches!(&facility.service, FacilityServiceKind::Food))
        }
        GuestVisitPurpose::Drink => {
            affordances.is_some_and(|affordances| {
                affordances.contains_all(WorldObjectAffordanceFlags::DRINK)
            }) || facility
                .is_some_and(|facility| matches!(&facility.service, FacilityServiceKind::Drink))
        }
        GuestVisitPurpose::Restroom => facility
            .is_some_and(|facility| matches!(&facility.service, FacilityServiceKind::Toilet)),
        GuestVisitPurpose::Rest => affordances
            .is_some_and(|affordances| affordances.contains_all(WorldObjectAffordanceFlags::REST)),
        GuestVisitPurpose::Education => {
            affordances.is_some_and(|affordances| {
                affordances.contains_all(WorldObjectAffordanceFlags::LEARN)
            }) || facility
                .is_some_and(|facility| matches!(&facility.service, FacilityServiceKind::Education))
        }
        GuestVisitPurpose::Shop => {
            affordances.is_some_and(|affordances| {
                affordances.contains_all(WorldObjectAffordanceFlags::BUY)
            }) || facility
                .is_some_and(|facility| matches!(&facility.service, FacilityServiceKind::Gift))
        }
        GuestVisitPurpose::Show => {
            facility.is_some_and(|facility| matches!(&facility.service, FacilityServiceKind::Show))
        }
        GuestVisitPurpose::Tour => {
            affordances.is_some_and(|affordances| {
                affordances.contains_all(WorldObjectAffordanceFlags::BOARD)
            }) || facility
                .is_some_and(|facility| matches!(&facility.service, FacilityServiceKind::Transport))
        }
        GuestVisitPurpose::Exit => false,
    }
}
