use avian3d::prelude::RigidBody;
use bevy::{gltf::Gltf, platform::collections::HashSet, prelude::*};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::simulation_time::simulation_clock_types::{ZooCalendar, ZooClock};
use openzt2_world_state::world_spawn::persistent_id_types::{PersistentId, PersistentIdAllocator};
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::economy::account_transaction_types::{
    Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
};
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::person_name_selection_and_resolution::choose_generated_person_name_rows_from_authored_pool;

use super::{
    staff_employment_types::{Employment, Staff, StaffRole},
    staff_lifecycle_messages::{HireStaffRequest, StaffHired},
    staff_presentation_variant_selection::{
        attach_staff_presentation_variant, staff_presentation_variant,
    },
};

#[derive(Component)]
pub(super) struct PendingEmployeePlacement {
    hire: HireStaffRequest,
    body: Handle<ScenePrefabAsset>,
}

#[derive(Component)]
pub(super) struct HirePaid;

#[derive(Component)]
pub(super) struct EmployeePlacementIdentity(PersistentId);

pub(super) fn request_staff_hire_payment(
    mut commands: Commands,
    mut requests: MessageReader<HireStaffRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    pending: Query<&PendingEmployeePlacement>,
    roots: Query<Entity, (With<WorldRoot>, With<PersistentIdAllocator>)>,
    mut transactions: MessageWriter<TransactionRequest>,
) {
    if requests.is_empty() {
        return;
    }
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    let hire_key = |hire: &HireStaffRequest| {
        (
            hire.role,
            hire.object,
            hire.position.to_array().map(f32::to_bits),
        )
    };
    let mut accepted: HashSet<_> = pending
        .iter()
        .map(|operation| hire_key(&operation.hire))
        .collect();
    for &hire in requests.read() {
        if !accepted.insert(hire_key(&hire)) {
            continue;
        }
        let Some(role) = catalogue
            .find_staff(hire.role)
            .filter(|role| role.object == hire.object)
        else {
            continue;
        };
        let Some(object) = catalogue.find_object(role.object) else {
            continue;
        };
        let Some(body) = catalogue.scene(object.prefab) else {
            continue;
        };
        let price = Money(object.price_cents);
        if price < Money::ZERO {
            continue;
        }
        let mut operation = commands.spawn((
            PendingEmployeePlacement { hire, body },
            WorldMember { root },
        ));
        if price == Money::ZERO {
            operation.insert(HirePaid);
        } else {
            transactions.write(TransactionRequest {
                operation: operation.id(),
                debit: Account::Zoo,
                credit: Account::External,
                amount: price,
                kind: TransactionKind::StaffHire,
                subject: None,
            });
        }
    }
}

pub(super) fn accept_paid_employee_placements(
    mut commands: Commands,
    mut completed: MessageReader<TransactionCompleted>,
    pending: Query<(), (With<PendingEmployeePlacement>, Without<HirePaid>)>,
) {
    for result in completed.read() {
        if result.kind == TransactionKind::StaffHire && pending.contains(result.operation) {
            commands.entity(result.operation).insert_if_new(HirePaid);
        }
    }
}

pub(super) fn spawn_paid_employees_from_ready_prefabs(
    mut commands: Commands,
    mut pending: Query<
        (
            Entity,
            &mut PendingEmployeePlacement,
            &WorldMember,
            Option<&EmployeePlacementIdentity>,
        ),
        With<HirePaid>,
    >,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    clock: Res<ZooClock>,
    calendar: Res<ZooCalendar>,
    mut identifiers: Query<&mut PersistentIdAllocator>,
    mut hired: MessageWriter<StaffHired>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (operation, pending, member, identity) in &mut pending {
        let hire = pending.hire;
        let Some(role) = catalogue.find_staff(hire.role) else {
            commands.entity(operation).despawn();
            continue;
        };
        let Some(&EmployeePlacementIdentity(identifier)) = identity else {
            let Some(identifier) = identifiers
                .get_mut(member.root)
                .ok()
                .and_then(|mut allocator| allocator.allocate().ok())
            else {
                commands.entity(operation).despawn();
                continue;
            };
            if let Some((_, body)) = staff_presentation_variant(role, identifier.0, catalogue) {
                pending
                    .map_unchanged(|pending| &mut pending.body)
                    .set_if_neq(body);
            }
            commands
                .entity(operation)
                .insert(EmployeePlacementIdentity(identifier));
            continue;
        };
        let Some(prefab) = prefabs.get(&pending.body) else {
            continue;
        };
        if first_missing_prefab_collider_model_asset_id(prefab, &models).is_some() {
            continue;
        }
        let variant = staff_presentation_variant(role, identifier.0, catalogue)
            .filter(|(_, drawn_body)| drawn_body.id() == pending.body.id())
            .map(|(variant, _)| variant);
        let name = variant
            .and_then(|variant| {
                choose_generated_person_name_rows_from_authored_pool(
                    catalogue,
                    variant.name_pool,
                    identifier.0,
                )
            })
            .or_else(|| {
                choose_generated_person_name_rows_from_authored_pool(
                    catalogue,
                    role.name_pool,
                    identifier.0,
                )
            });
        let employee = spawn_loaded_scene_prefab_as_world_instance(
            &mut commands,
            prefab,
            pending.body.clone(),
            member.root,
            hire.object,
            identifier,
            Transform::from_translation(hire.position),
            true,
            None,
            RigidBody::Dynamic,
        );
        if let Some(variant) = variant {
            attach_staff_presentation_variant(
                &mut commands,
                employee,
                variant,
                identifier.0,
                catalogue,
            );
        }
        super::spawned_staff_identification::attach_staff_virtual_nodes(
            &mut commands,
            employee,
            role,
        );
        let mut employee_commands = commands.entity(employee);
        employee_commands.insert((
            Staff,
            StaffRole(hire.role),
            Employment {
                wage: Money(i64::from(role.wage_cents_per_month)),
                hired_tick: clock.tick,
                hired_month_ordinal: u32::from(calendar.year) * 12
                    + u32::from(calendar.month.saturating_sub(1)),
            },
        ));
        if let Some(name) = name {
            employee_commands.insert(name);
        }
        hired.write(StaffHired {
            staff: employee,
            role: hire.role,
        });
        commands.entity(operation).despawn();
    }
}

pub(super) fn discard_rejected_employee_placements(
    mut commands: Commands,
    mut rejected: MessageReader<TransactionRejected>,
    pending: Query<(), (With<PendingEmployeePlacement>, Without<HirePaid>)>,
) {
    for result in rejected.read() {
        if result.kind == TransactionKind::StaffHire && pending.contains(result.operation) {
            commands.entity(result.operation).try_despawn();
        }
    }
}
