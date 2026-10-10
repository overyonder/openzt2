use avian3d::prelude::RigidBody;
use bevy::{gltf::Gltf, platform::collections::HashSet, prelude::*};
use openzt2_game_data::AssetId;

use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::account_transaction_types::Account;
use crate::plugins::economy::account_transaction_types::TransactionCompleted;
use crate::plugins::economy::account_transaction_types::TransactionKind;
use crate::plugins::economy::account_transaction_types::TransactionRejected;
use crate::plugins::economy::account_transaction_types::TransactionRequest;
use crate::plugins::economy::money_types::Money;
use crate::plugins::person_name_selection_and_resolution::choose_generated_person_name_rows_from_authored_pool;
use crate::plugins::simulation_time::simulation_clock_types::ZooCalendar;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::{PersistentId, PersistentIdAllocator};
use crate::plugins::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use crate::plugins::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    staff_assignment_types::StaffAssignment,
    staff_employment_types::{AvailableForWork, Employment, PendingStaffHire, Staff, StaffRole},
    staff_lifecycle_messages::{HireStaffRequest, StaffHired},
    staff_presentation_variant_selection::{
        attach_staff_presentation_variant, staff_presentation_variant,
    },
};

/// Keeps the selected prefab loaded while payment and spawning are pending.
#[derive(Component)]
pub(super) struct PendingStaffHirePrefab(Handle<ScenePrefabAsset>);

/// The hire debit completed; prefab hydration may now create the employee.
#[derive(Component)]
pub(super) struct PaidStaffHire;

pub(in crate::plugins::staff) fn request_staff_hiring(
    mut commands: Commands,
    mut requests: MessageReader<HireStaffRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    pending: Query<&PendingStaffHire>,
    roots: Query<(Entity, &WorldRoot)>,
    mut transactions: MessageWriter<TransactionRequest>,
    mut accepted: Local<HashSet<(AssetId, [u32; 3])>>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Some((root, _)) = roots.iter().next() else {
        return;
    };
    accepted.clear();
    for request in requests.read() {
        let key = (request.role, request.position.to_array().map(f32::to_bits));
        if pending.iter().any(|operation| {
            operation.role == request.role
                && operation.position.to_array().map(f32::to_bits)
                    == request.position.to_array().map(f32::to_bits)
        }) || !accepted.insert(key)
        {
            continue;
        }
        let Some(role) = catalogue.find_staff(request.role) else {
            continue;
        };
        let Some(object) = catalogue.find_object(role.object) else {
            continue;
        };
        let Some(prefab) = catalogue.scene(object.prefab) else {
            continue;
        };
        let price = Money(object.price_cents);
        if price.0 <= 0 {
            continue;
        }
        let operation = commands
            .spawn((
                PendingStaffHire {
                    role: request.role,
                    position: request.position,
                },
                PendingStaffHirePrefab(prefab),
                WorldMember { root },
            ))
            .id();
        transactions.write(TransactionRequest {
            operation,
            debit: Account::Zoo,
            credit: Account::External,
            amount: price,
            kind: TransactionKind::StaffHire,
            subject: None,
        });
    }
}

pub(in crate::plugins::staff) fn mark_paid_staff_hires_for_prefab_spawning(
    mut commands: Commands,
    mut completed: MessageReader<TransactionCompleted>,
    pending: Query<(), (With<PendingStaffHire>, Without<PaidStaffHire>)>,
    mut handled: Local<HashSet<Entity>>,
) {
    handled.clear();
    for result in completed.read() {
        if handled.insert(result.operation) && pending.get(result.operation).is_ok() {
            commands.entity(result.operation).insert(PaidStaffHire);
        }
    }
}

/// Persistent id allocated once payment completes, so the employee's drawn
/// look can load its own body before spawning.
#[derive(Component)]
pub(super) struct PaidStaffHireIdentity(PersistentId);

#[allow(clippy::too_many_arguments)]
pub(in crate::plugins::staff) fn spawn_paid_staff_hires_from_ready_prefabs(
    mut commands: Commands,
    pending: Query<
        (
            Entity,
            &PendingStaffHire,
            &PendingStaffHirePrefab,
            &WorldMember,
            Option<&PaidStaffHireIdentity>,
        ),
        With<PaidStaffHire>,
    >,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    clock: Res<ZooClock>,
    calendar: Res<ZooCalendar>,
    mut ids: ResMut<PersistentIdAllocator>,
    mut hired: MessageWriter<StaffHired>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (operation, request, prefab_handle, member, identity) in &pending {
        let Some(role) = catalogue.find_staff(request.role) else {
            commands.entity(operation).despawn();
            continue;
        };
        let Some(PaidStaffHireIdentity(id)) = identity else {
            let Ok(id) = ids.allocate(member.root) else {
                commands.entity(operation).despawn();
                continue;
            };
            let mut operation_commands = commands.entity(operation);
            operation_commands.insert(PaidStaffHireIdentity(id));
            if let Some((_, prefab)) = staff_presentation_variant(role, id.0, catalogue) {
                operation_commands.insert(PendingStaffHirePrefab(prefab));
            }
            continue;
        };
        let id = *id;
        let Some(prefab) = prefabs.get(&prefab_handle.0) else {
            continue;
        };
        if first_missing_prefab_collider_model_asset_id(prefab, &models).is_some() {
            continue;
        }
        let variant = staff_presentation_variant(role, id.0, catalogue).map(|(variant, _)| variant);
        let name = variant
            .and_then(|variant| {
                choose_generated_person_name_rows_from_authored_pool(
                    catalogue,
                    AssetId(variant.name_pool.0),
                    id.0,
                )
            })
            .or_else(|| {
                choose_generated_person_name_rows_from_authored_pool(
                    catalogue,
                    AssetId(role.name_pool.0),
                    id.0,
                )
            });
        let staff = spawn_loaded_scene_prefab_as_world_instance(
            &mut commands,
            prefab,
            prefab_handle.0.clone(),
            member.root,
            AssetId(role.object.0),
            id,
            Transform::from_translation(request.position),
            true,
            None,
            RigidBody::Dynamic,
        );
        if let Some(variant) = variant {
            attach_staff_presentation_variant(&mut commands, staff, variant, id.0, catalogue);
        }
        let mut staff_commands = commands.entity(staff);
        staff_commands.insert((
            Staff,
            StaffRole(request.role),
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
        if let Some(name) = name {
            staff_commands.insert(name);
        }
        hired.write(StaffHired {
            staff,
            role: request.role,
        });
        commands.entity(operation).despawn();
    }
}

pub(in crate::plugins::staff) fn discard_rejected_staff_hires(
    mut commands: Commands,
    mut rejected: MessageReader<TransactionRejected>,
    pending: Query<(), With<PendingStaffHire>>,
) {
    for result in rejected.read() {
        if pending.get(result.operation).is_ok() {
            commands.entity(result.operation).despawn();
        }
    }
}
