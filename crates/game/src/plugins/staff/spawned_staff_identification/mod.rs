use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::money_types::Money;
use crate::plugins::person_name_selection_and_resolution::choose_generated_person_name_rows_from_authored_pool;
use crate::plugins::simulation_time::simulation_clock_types::ZooCalendar;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::{
    staff_assignment_types::StaffAssignment,
    staff_employment_types::{AvailableForWork, Employment, Staff, StaffRole},
};

/// Initializes staff loaded from world prefabs. Definitions can identify
/// either a staff role or its object.
pub(in crate::plugins::staff) fn identify_spawned_staff_from_world_definition(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    clock: Res<ZooClock>,
    calendar: Res<ZooCalendar>,
    spawned_entities: Query<
        (Entity, &DefinitionId, &PersistentId),
        (Added<DefinitionId>, Without<Staff>),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, definition, persistent_identifier) in &spawned_entities {
        let Some(role) = catalogue
            .find_staff(definition.0)
            .or_else(|| catalogue.find_staff_by_object(definition.0))
        else {
            continue;
        };
        let role_identifier = AssetId(role.id.0);
        let person_name = choose_generated_person_name_rows_from_authored_pool(
            catalogue,
            AssetId(role.name_pool.0),
            persistent_identifier.0,
        );
        let mut staff_commands = commands.entity(entity);
        staff_commands.insert((
            Staff,
            StaffRole(role_identifier),
            Employment {
                wage: Money(i64::from(role.wage_cents_per_month)),
                hired_tick: clock.tick,
                hired_month_ordinal: u32::from(calendar.year)
                    .saturating_mul(12)
                    .saturating_add(u32::from(calendar.month.saturating_sub(1))),
            },
            StaffAssignment::default(),
            AvailableForWork,
        ));
        if let Some(person_name) = person_name {
            staff_commands.insert(person_name);
        }
    }
}
