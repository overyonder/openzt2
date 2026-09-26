#[cfg(test)]
use super::authored_transaction_lowering::lower_object_transactions;
#[cfg(test)]
use super::interaction_container_source_lowering::lower_authored_world_object_interaction_slots;
use super::source_element_tree_search::authored_type_family_component_attribute;
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{id, money_cents};
#[cfg(test)]
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::facilities_and_maintenance::{
    FacilityDefinition, FacilityPaymentTrigger, FacilityServiceKind,
};
use openzt2_game_data::world_definitions::staff_management::StaffRoleKind;
use openzt2_game_data::world_definitions::world_objects::WorldObjectInteractionSlotDefinition;
use openzt2_game_data::AssetId;
use std::collections::BTreeSet;

/// Guest behavior controls payment and service timing for commerce buildings.
pub(super) fn bind_authored_commerce_facility(
    record: &RecordView<'_, '_>,
    abstract_binder: bool,
    interaction_slots: &[WorldObjectInteractionSlotDefinition],
    transactions: &[openzt2_game_data::world_definitions::economy_transactions::EconomyTransactionDefinition],
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if abstract_binder {
        return Ok(());
    }
    let commerce_building = authored_type_family_component_attribute(
        record,
        "BFAIEntityDataShared",
        "b_CommerceBuilding",
    )
    .is_some_and(|value| {
        matches!(
            canonicalize_source_document_record_key(value).as_str(),
            "true" | "1" | "yes"
        )
    });
    if !commerce_building {
        return Ok(());
    }
    let Some(buy_item) = transactions
        .iter()
        .find(|transaction| transaction.name == AssetId::from_key("buy_item"))
    else {
        return Ok(());
    };
    let service = match buy_item.category {
        category if category == AssetId::from_key("concessions_food_drink") => {
            FacilityServiceKind::Food
        }
        category if category == AssetId::from_key("concessions_gifts") => FacilityServiceKind::Gift,
        category => {
            return Err(BindError::record(
                record,
                format!(
                    "commerce building sells through unmapped transaction category {}",
                    category.to_lowercase_hexadecimal_string()
                ),
            ));
        }
    };
    // These reservation tags are used by the guest purchase tasks.
    let purchase_reservation_tags = [
        "use_cart",
        "use_stand",
        "use_giftshop",
        "use_photobooth",
        "use_restaurant",
        "use_ride",
        "use_skytower",
        "use_kiosk",
    ]
    .map(AssetId::from_key);
    let service_container_tags = interaction_slots
        .iter()
        .filter(|slot| {
            !slot.is_queue
                && slot.capacity > 0
                && purchase_reservation_tags.contains(&slot.reservation_tag)
        })
        .map(|slot| slot.reservation_tag)
        .collect::<BTreeSet<_>>();
    if service_container_tags.len() > 1 {
        return Err(BindError::record(
            record,
            "commerce building has more than one distinct guest service container; the relevant service capacity is ambiguous",
        ));
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "capacity sum clamped to the u16 stored range"
    )]
    let capacity = interaction_slots
        .iter()
        .filter(|slot| !slot.is_queue && service_container_tags.contains(&slot.reservation_tag))
        .map(|slot| u32::from(slot.capacity))
        .sum::<u32>()
        .clamp(0, u32::from(u16::MAX)) as u16;
    let authored_price = if buy_item.cost_choices.is_empty() {
        buy_item.cost
    } else {
        *buy_item
            .cost_choices
            .get(buy_item.initial_cost_index)
            .ok_or_else(|| {
                BindError::record(
                    record,
                    "commerce sale price index is absent from the authored cost choices",
                )
            })?
    };
    output.document.facilities.push(FacilityDefinition {
        id: id(&format!("facility/{}", record.key)),
        object: id(record.key),
        service,
        capacity,
        // The guest buy behavior determines service timing.
        service_ticks: 0,
        payment_trigger: FacilityPaymentTrigger::AuthoredBehavior,
        price_cents: money_cents(f64::from(authored_price), record)?,
        staffing: StaffRoleKind::None,
        inventory_capacity: 0,
        inventory_units_per_service: 0,
        inventory_restock_per_zoo_day: 0,
    });
    Ok(())
}

#[cfg(test)]
mod authored_commerce_facility_registration_tests {

    use super::*;
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocument;
    use crate::assets::source_document::path::AssetPath;

    // The service container and add_user transaction are inherited from snackcart.
    fn fruitcup_source_documents() -> [OrderedSourceDocument; 2] {
        [
            parse_blue_fang_source_document(
                AssetPath::new("entities/objects/buildings/ai/snackcart.xml"),
                br#"<BFTypedBinder binderType="snackcart" abstract="true">
                     <types><entity><building><snackcart/></building></entity></types>
                     <binder>
                       <BFNamedBinder binderName="Use_Cart"><instance><BFGEntityContainer>
                         <slots><BFGEntityContainerSlot targetNode="Dock_Adult" capacity="1"/></slots>
                         <queue><BFGEntityContainerSlot targetNode="Dock_Adult" capacity="5"/></queue>
                       </BFGEntityContainer></instance></BFNamedBinder>
                       <BFBinder><instance><ZTEconomyComponent cost="0">
                         <ZTTransaction name="add_user" cost="1" type="addUser" category="user"/>
                       </ZTEconomyComponent></instance></BFBinder>
                     </binder>
                   </BFTypedBinder>"#,
            )
            .expect("source document"),
            parse_blue_fang_source_document(
                AssetPath::new("entities/objects/buildings/ai/snackcart_fruitcup_df.xml"),
                br#"<BFTypedBinder binderType="snackcart_fruitcup_df">
                     <types><entity><building><snackcart><snackcart_fruitcup><snackcart_fruitcup_df/></snackcart_fruitcup></snackcart></building></entity></types>
                     <shared>
                       <BFAIEntityDataShared s_ItemsSold="FruitCup" b_CommerceBuilding="true"/>
                     </shared>
                     <binder>
                       <BFBinder><instance><ZTEconomyComponent cost="400">
                         <ZTTransaction name="build" costType="parent" type="debit" category="construction"/>
                         <ZTTransaction name="upkeep" cost="20" period="monthly" type="debit" category="upkeep"/>
                         <ZTTransaction name="Buy_Item" costIndex="1" costChoice="6 12 15" type="debit" category="concessions_food_drink" nextTransaction="add_user"/>
                       </ZTEconomyComponent></instance></BFBinder>
                     </binder>
                   </BFTypedBinder>"#,
            )
            .expect("source document"),
        ]
    }

    #[test]
    fn fruitcup_cart_registers_facility_from_inherited_commerce_source() {
        let documents = fruitcup_source_documents();
        let index = SourceIndex::build(&documents).expect("source index");
        let record = index.find("snackcart_fruitcup_df").expect("cart");
        // The container is inherited, not repeated on the concrete binder.
        let slots = lower_authored_world_object_interaction_slots(&record).expect("slots");
        assert_eq!(
            slots.len(),
            2,
            "Use_Cart container must arrive through the family base"
        );
        let transactions = lower_object_transactions(&record).expect("transactions");
        let mut tables = WorldDefinitionLoweringTables::default();
        bind_authored_commerce_facility(&record, false, &slots, &transactions, &mut tables)
            .expect("facility registration");
        assert_eq!(tables.document.facilities.len(), 1);
        let facility = &tables.document.facilities[0];
        assert_eq!(facility.id, id("facility/snackcart_fruitcup_df"));
        assert_eq!(facility.object, id("snackcart_fruitcup_df"));
        assert_eq!(facility.service, FacilityServiceKind::Food);
        assert_eq!(
            facility.capacity, 1,
            "only the inherited service slot counts; the five queue places are waiting capacity"
        );
        assert_eq!(
            facility.payment_trigger,
            FacilityPaymentTrigger::AuthoredBehavior
        );
        assert_eq!(facility.service_ticks, 0);
        assert_eq!(
            facility.price_cents, 1_200,
            "costIndex 1 selects the authored moderate price"
        );
        assert_eq!(facility.staffing, StaffRoleKind::None);
        assert_eq!(facility.inventory_capacity, 0);
        assert_eq!(facility.inventory_units_per_service, 0);
    }
}
