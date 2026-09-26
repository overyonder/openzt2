use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffRoleKind, AssetId};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::{
    facility_economy_types::FacilityProfit,
    facility_economy_types::{
        Inventory, MonthlyUpkeep, Price, RequiredFacilityStaff, ServiceFacility,
    },
    money_types::Money,
};

#[derive(Component)]
pub(super) struct ObjectEconomyFactsResolved;

#[derive(Component)]
pub(super) struct ServiceFacilityFactsResolved;

pub(super) fn hydrate_authored_monthly_upkeep_for_new_world_objects(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    objects: Query<(Entity, &DefinitionId), Without<ObjectEconomyFactsResolved>>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (entity, definition_id) in &objects {
        let Some(object_definition) = world_definitions.find_object(definition_id.0) else {
            commands.entity(entity).insert(ObjectEconomyFactsResolved);
            continue;
        };

        let monthly_upkeep = i64::from(object_definition.upkeep_cents_per_month);
        if !object_definition.transactions.is_empty() {
            commands.entity(entity).insert(FacilityProfit::default());
        }
        if monthly_upkeep != 0 {
            commands.entity(entity).insert((
                MonthlyUpkeep(Money(monthly_upkeep)),
                ObjectEconomyFactsResolved,
            ));
        } else {
            commands.entity(entity).insert(ObjectEconomyFactsResolved);
        }
    }
}

/// Reads one definition's source-compiled price directly from the loaded
/// world catalogue. Absence remains absence: callers decide whether a missing
/// price hides presentation or rejects an operation.
pub(crate) fn find_authored_object_or_placeable_price(
    world_definitions: WorldDefinitionsView<'_>,
    definition: AssetId,
) -> Option<Money> {
    world_definitions
        .find_object(definition)
        .map(|object| Money(object.price_cents))
        .or_else(|| {
            world_definitions
                .find_placeable(definition)
                .map(|placeable| Money(placeable.price_cents))
        })
}

pub(super) fn hydrate_authored_service_facility_facts_for_new_world_objects(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    facilities: Query<(Entity, &DefinitionId), Without<ServiceFacilityFactsResolved>>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (entity, definition_id) in &facilities {
        let Some(facility_definition) = world_definitions.find_facility_by_object(definition_id.0)
        else {
            commands.entity(entity).insert(ServiceFacilityFactsResolved);
            continue;
        };

        let mut entity_commands = commands.entity(entity);
        entity_commands.insert((
            Price(Money(i64::from(facility_definition.price_cents))),
            ServiceFacility {
                definition: AssetId(facility_definition.id.0),
                capacity: facility_definition.capacity,
                occupied: 0,
                payment_trigger: facility_definition.payment_trigger,
            },
            ServiceFacilityFactsResolved,
        ));
        if let Some(required_staff) =
            map_authored_staff_kind_to_supported_facility_operator(&facility_definition.staffing)
        {
            entity_commands.insert(RequiredFacilityStaff(required_staff));
        }
        if facility_definition.inventory_capacity > 0 {
            entity_commands.insert(Inventory {
                available: facility_definition.inventory_capacity,
                reserved: 0,
                capacity: facility_definition.inventory_capacity,
            });
        }
    }
}

fn map_authored_staff_kind_to_supported_facility_operator(
    staff_kind: &StaffRoleKind,
) -> Option<StaffRoleKind> {
    match staff_kind {
        StaffRoleKind::None => None,
        StaffRoleKind::Keeper => Some(StaffRoleKind::Keeper),
        StaffRoleKind::Maintenance => Some(StaffRoleKind::Maintenance),
        StaffRoleKind::Educator => Some(StaffRoleKind::Educator),
        StaffRoleKind::Veterinarian => Some(StaffRoleKind::Veterinarian),
        StaffRoleKind::Entertainer => Some(StaffRoleKind::Entertainer),
        StaffRoleKind::Trainer
        | StaffRoleKind::Presenter
        | StaffRoleKind::Paleontologist
        | StaffRoleKind::Recovery => None,
    }
}

#[cfg(test)]
mod authored_service_facility_hydration_tests {
    use super::*;
    use openzt2_game_data::world_definitions::{
        document::WorldDefinitionDocument,
        facilities_and_maintenance::{
            FacilityDefinition, FacilityPaymentTrigger, FacilityServiceKind,
        },
    };
    use openzt2_game_data::AssetId;

    fn cart_object() -> AssetId {
        AssetId::from_key("snackcart_fruitcup_df")
    }

    fn fruitcup_facility_document() -> WorldDefinitionDocument {
        WorldDefinitionDocument {
            facilities: vec![FacilityDefinition {
                id: AssetId::from_key("facility/snackcart_fruitcup_df"),
                object: cart_object(),
                service: FacilityServiceKind::Food,
                capacity: 1,
                service_ticks: 0,
                payment_trigger: FacilityPaymentTrigger::AuthoredBehavior,
                price_cents: 1_200,
                staffing: StaffRoleKind::None,
                inventory_capacity: 0,
                inventory_units_per_service: 0,
                inventory_restock_per_zoo_day: 0,
            }],
            ..default()
        }
    }

    fn create_application_with_fruitcup_facility() -> App {
        // The system reads only the Assets resource; no AssetServer or asset
        // plugin is involved, matching the established test fixture pattern.
        let mut application = App::new();
        application
            .insert_resource(Assets::<WorldDefinitionAsset>::default())
            .init_resource::<WorldDefinitions>()
            .add_systems(
                Update,
                hydrate_authored_service_facility_facts_for_new_world_objects,
            );
        let document = fruitcup_facility_document();
        let handle = application
            .world_mut()
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(document.clone()));
        application
            .world_mut()
            .resource_mut::<WorldDefinitions>()
            .index_test_document(handle, &document);
        application
    }

    #[test]
    fn placed_commerce_cart_hydrates_price_service_facility_and_behavior_owned_payment_trigger() {
        let mut application = create_application_with_fruitcup_facility();
        let cart = application
            .world_mut()
            .spawn(DefinitionId(cart_object()))
            .id();

        application.update();

        let price = application
            .world()
            .get::<Price>(cart)
            .expect("authored price");
        assert_eq!(price.0, Money(1_200));
        let facility = application
            .world()
            .get::<ServiceFacility>(cart)
            .expect("commerce cart is a live service facility");
        assert_eq!(facility.capacity, 1);
        assert_eq!(facility.occupied, 0);
        assert_eq!(
            facility.payment_trigger,
            FacilityPaymentTrigger::AuthoredBehavior,
            "the authored buy behavior set owns payment; the hydration must carry that ownership into the live component"
        );
    }
}
