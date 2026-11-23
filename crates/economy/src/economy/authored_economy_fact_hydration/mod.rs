use bevy::prelude::*;
use openzt2_game_data::world_definitions::{
    economy_transactions::EconomyTransactionKind, staff_management::StaffRoleKind,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::{
    behavior_transaction_state::EconomyUsage,
    facility_economy_types::FacilityProfit,
    facility_economy_types::{
        Inventory, MonthlyUpkeep, Price, RequiredFacilityStaff, ServiceFacility,
    },
    money_types::Money,
};

pub(super) fn hydrate_authored_monthly_upkeep_for_new_world_objects(
    mut commands: Commands,
    assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut objects: Query<(
        Entity,
        &DefinitionId,
        Has<FacilityProfit>,
        Has<EconomyUsage>,
        Option<&mut MonthlyUpkeep>,
    )>,
    changed_definitions: Query<Entity, Changed<DefinitionId>>,
    retired_objects: Query<(), (With<MonthlyUpkeep>, Without<DefinitionId>)>,
    mut removed_definitions: RemovedComponents<DefinitionId>,
) {
    let Some(definitions) = active_definitions.get(&assets) else {
        return;
    };
    for entity in removed_definitions.read() {
        if retired_objects.contains(entity) {
            commands.entity(entity).try_remove::<MonthlyUpkeep>();
        }
    }
    let dirty = if active_definitions.is_changed() {
        objects
            .iter()
            .map(|(entity, ..)| entity)
            .collect::<Vec<_>>()
    } else {
        changed_definitions.iter().collect::<Vec<_>>()
    };
    let mut objects = objects.iter_many_mut(dirty);
    while let Some(row) = objects.fetch_next() {
        let Ok((entity, definition, has_profit, has_usage, upkeep)) = row else {
            continue;
        };
        let definition = definitions.find_object(definition.0);
        let monthly = definition
            .map(|definition| definition.upkeep_cents_per_month)
            .filter(|&upkeep| upkeep != 0)
            .map(|upkeep| MonthlyUpkeep(Money(i64::from(upkeep))));
        match (upkeep, monthly) {
            (Some(mut upkeep), Some(monthly)) => {
                upkeep.set_if_neq(monthly);
            }
            (None, Some(monthly)) => {
                commands.entity(entity).insert(monthly);
            }
            (Some(_), None) => {
                commands.entity(entity).try_remove::<MonthlyUpkeep>();
            }
            (None, None) => {}
        }
        if let Some(definition) = definition {
            if !has_profit && !definition.transactions.is_empty() {
                commands
                    .entity(entity)
                    .try_insert_if_new(FacilityProfit::default());
            }
            if !has_usage
                && definition.transactions.iter().any(|transaction| {
                    matches!(
                        transaction.kind,
                        EconomyTransactionKind::AddUser | EconomyTransactionKind::RemoveUser
                    )
                })
            {
                commands
                    .entity(entity)
                    .try_insert_if_new(EconomyUsage::default());
            }
        }
    }
}

pub(super) fn hydrate_authored_service_facility_facts_for_new_world_objects(
    mut commands: Commands,
    assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut facilities: Query<(
        Entity,
        Ref<DefinitionId>,
        Has<Price>,
        Option<&mut ServiceFacility>,
        Has<Inventory>,
        Option<&mut RequiredFacilityStaff>,
    )>,
    changed_definitions: Query<Entity, Changed<DefinitionId>>,
    retired_facilities: Query<
        (),
        (
            Or<(With<ServiceFacility>, With<RequiredFacilityStaff>)>,
            Without<DefinitionId>,
        ),
    >,
    mut removed_definitions: RemovedComponents<DefinitionId>,
) {
    let Some(definitions) = active_definitions.get(&assets) else {
        return;
    };
    for entity in removed_definitions.read() {
        if retired_facilities.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<(ServiceFacility, RequiredFacilityStaff)>();
        }
    }
    let dirty = if active_definitions.is_changed() {
        facilities
            .iter()
            .map(|(entity, ..)| entity)
            .collect::<Vec<_>>()
    } else {
        changed_definitions.iter().collect::<Vec<_>>()
    };
    let mut facilities = facilities.iter_many_mut(dirty);
    while let Some(row) = facilities.fetch_next() {
        let Ok((entity, definition_id, has_price, service, has_inventory, required_staff)) = row
        else {
            continue;
        };
        let definition = definitions.find_facility_by_object(definition_id.0);
        let staff = definition
            .and_then(|definition| {
                map_authored_staff_kind_to_supported_facility_operator(&definition.staffing)
            })
            .map(RequiredFacilityStaff);
        match (required_staff, staff) {
            (Some(mut current), Some(staff)) => {
                current.set_if_neq(staff);
            }
            (None, Some(staff)) => {
                commands.entity(entity).insert(staff);
            }
            (Some(_), None) => {
                commands
                    .entity(entity)
                    .try_remove::<RequiredFacilityStaff>();
            }
            (None, None) => {}
        }
        let Some(definition) = definition else {
            if service.is_some() {
                commands.entity(entity).try_remove::<ServiceFacility>();
            }
            continue;
        };
        let next = ServiceFacility {
            definition: definition.id,
            capacity: definition.capacity,
            payment_trigger: definition.payment_trigger,
        };
        if let Some(mut service) = service {
            if !definition_id.is_added() {
                service.set_if_neq(next);
            }
        } else {
            commands.entity(entity).insert(next);
        }
        if !has_price {
            commands
                .entity(entity)
                .insert(Price(Money(i64::from(definition.price_cents))));
        }
        if !has_inventory && definition.inventory_capacity > 0 {
            commands.entity(entity).insert(Inventory {
                available: definition.inventory_capacity,
                reserved: 0,
                capacity: definition.inventory_capacity,
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
pub(super) fn economy_test_simulation_timing()
-> openzt2_game_data::world_definitions::simulation_time::SimulationTimingDefinition {
    use openzt2_game_data::world_definitions::simulation_time::{
        SimulationCalendarLeapPolicy, SimulationCalendarPolicy, SimulationTimingDefinition,
    };
    SimulationTimingDefinition {
        fixed_hz: 20,
        ticks_per_day: 100,
        speed_multipliers: Vec::new(),
        calendar: SimulationCalendarPolicy {
            epoch_year: 1,
            epoch_month: 1,
            epoch_day: 1,
            month_lengths: [30; 12],
            leap: SimulationCalendarLeapPolicy::None,
        },
    }
}

#[cfg(test)]
mod authored_service_facility_hydration_tests {
    use super::*;
    use openzt2_game_data::AssetId;
    use openzt2_game_data::world_definitions::{
        document::WorldDefinitionDocument,
        facilities_and_maintenance::{
            FacilityDefinition, FacilityPaymentTrigger, FacilityServiceKind,
        },
    };

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
            simulation_timing: Some(economy_test_simulation_timing()),
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
        assert_eq!(
            facility.payment_trigger,
            FacilityPaymentTrigger::AuthoredBehavior,
            "the authored buy behavior set owns payment; the hydration must carry that ownership into the live component"
        );
    }
    #[test]
    fn restored_capacity_inventory_and_price_survive_initial_hydration() {
        let mut application = create_application_with_fruitcup_facility();
        let facility = ServiceFacility {
            definition: AssetId::from_key("facility/snackcart_fruitcup_df"),
            capacity: 3,
            payment_trigger: FacilityPaymentTrigger::AuthoredBehavior,
        };
        let stock = Inventory {
            available: 6,
            reserved: 4,
            capacity: 10,
        };
        let cart = application
            .world_mut()
            .spawn((
                DefinitionId(cart_object()),
                Price(Money(700)),
                facility,
                stock,
            ))
            .id();
        application.update();
        assert_eq!(
            application.world().get::<Price>(cart),
            Some(&Price(Money(700)))
        );
        assert_eq!(
            application.world().get::<ServiceFacility>(cart),
            Some(&facility)
        );
        assert_eq!(application.world().get::<Inventory>(cart), Some(&stock));
    }
}
