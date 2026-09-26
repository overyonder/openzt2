use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::locomotion::locomotion_types::{Destination, NavAgent, NavFlags, Route};

use super::staff_employment_types::{Staff, StaffRole};

pub(in crate::plugins::staff) fn project_authored_staff_locomotion_onto_staff_entities(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut staff: Query<
        (
            Entity,
            Ref<StaffRole>,
            Option<&NavAgent>,
            Option<&mut Destination>,
            Option<&mut Route>,
        ),
        With<Staff>,
    >,
) {
    let definitions_changed = definitions.is_changed() || active_definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (staff_entity, staff_role, current_agent, destination, route) in &mut staff {
        if current_agent.is_some() && !definitions_changed && !staff_role.is_changed() {
            continue;
        }
        let Some(staff_role_definition) = definitions.find_staff(staff_role.0) else {
            continue;
        };
        let agent = NavAgent {
            radius_m: staff_role_definition.navigation_radius_m,
            max_speed_mps: staff_role_definition.move_speed_mps,
            // Staff movement has no authored acceleration limit.
            acceleration_mps2: f32::MAX,
            capabilities: NavFlags::STAFF,
        };
        if current_agent == Some(&agent) {
            continue;
        }
        if current_agent.is_some_and(|current| {
            current.radius_m != agent.radius_m || current.capabilities != agent.capabilities
        }) {
            if let Some(mut route) = route {
                route.clear();
            }
            if let Some(mut destination) = destination {
                destination.set_changed();
            }
        }
        commands.entity(staff_entity).insert(agent);
    }
}
