use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffRoleDefinition, AssetId};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::money_types::Money;
use crate::plugins::person_name_selection_and_resolution::choose_generated_person_name_rows_from_authored_pool;
use crate::plugins::simulation_time::simulation_clock_types::ZooCalendar;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use crate::plugins::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::{
    staff_assignment_types::StaffAssignment,
    staff_employment_types::{AvailableForWork, Employment, Staff, StaffRole},
    staff_presentation_variant_selection::{
        attach_staff_presentation_variant, staff_presentation_variant, staff_role_for_definition,
        StaffModelAnimationSet,
    },
};

/// Initializes staff loaded from world prefabs or restored from a save.
/// Definitions can identify either a staff role or its object. A restored
/// employee arrives with its saved [`Employment`], which is kept.
pub(in crate::plugins::staff) fn identify_spawned_staff_from_world_definition(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    clock: Res<ZooClock>,
    calendar: Res<ZooCalendar>,
    spawned_entities: Query<
        (
            Entity,
            &DefinitionId,
            &PersistentId,
            Option<&PrefabSourceAssetHandle>,
            Option<&StaffRole>,
            Has<StaffModelAnimationSet>,
        ),
        (Added<DefinitionId>, Without<Staff>),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, definition, persistent_identifier, body, restored_role, dressed) in
        &spawned_entities
    {
        // A restored employee carries its saved role; roles can share an object.
        let role = match restored_role {
            Some(role) => catalogue.find_staff(role.0),
            None => staff_role_for_definition(catalogue, definition.0),
        };
        let Some(role) = role else {
            continue;
        };
        let role_identifier = AssetId(role.id.0);
        // An authored record may pin its own body; only dress the drawn one.
        let variant = staff_presentation_variant(role, persistent_identifier.0, catalogue)
            .filter(|(_, prefab)| body.is_some_and(|body| body.0.id() == prefab.id()))
            .map(|(variant, _)| variant);
        // A restored employee is dressed when it spawns.
        if let Some(variant) = variant.filter(|_| !dressed) {
            attach_staff_presentation_variant(
                &mut commands,
                entity,
                variant,
                persistent_identifier.0,
                catalogue,
            );
        }
        let person_name = variant
            .and_then(|variant| {
                choose_generated_person_name_rows_from_authored_pool(
                    catalogue,
                    AssetId(variant.name_pool.0),
                    persistent_identifier.0,
                )
            })
            .or_else(|| {
                choose_generated_person_name_rows_from_authored_pool(
                    catalogue,
                    AssetId(role.name_pool.0),
                    persistent_identifier.0,
                )
            });
        let mut staff_commands = commands.entity(entity);
        staff_commands
            .insert((
                Staff,
                StaffRole(role_identifier),
                StaffAssignment::default(),
                AvailableForWork,
            ))
            .insert_if_new(Employment {
                wage: Money(i64::from(role.wage_cents_per_month)),
                hired_tick: clock.tick,
                hired_month_ordinal: u32::from(calendar.year)
                    .saturating_mul(12)
                    .saturating_add(u32::from(calendar.month.saturating_sub(1))),
            });
        if let Some(person_name) = person_name {
            staff_commands.insert(person_name);
        }
        attach_staff_virtual_nodes(&mut commands, entity, role);
    }
}

/// Behaviors find named points by attachment identity among the employee's
/// descendants, as they find authored model nodes.
pub(in crate::plugins::staff) fn attach_staff_virtual_nodes(
    commands: &mut Commands,
    staff: Entity,
    role: &StaffRoleDefinition,
) {
    for node in &role.virtual_nodes {
        commands.spawn((
            Transform::from_translation(Vec3::from_array(node.translation_m))
                .with_rotation(Quat::from_rotation_y(node.yaw_radians)),
            PrefabAuthoredAttachmentIdentifier(node.name),
            ChildOf(staff),
        ));
    }
}
