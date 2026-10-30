//! World state that authored behavior scripts read and change through
//! `queryObject` managers and entity tables, such as `CompostPoo.lua`.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorKilledTarget;
use openzt2_zoo_state::economy::account_transaction_types::{
    Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
};
use openzt2_zoo_state::economy::authored_transaction_amount::authored_currency_to_money;
use openzt2_zoo_state::maintenance::maintenance_types::CompostPooLevel;

/// Owns a zoo income requested by a behavior script until it settles.
#[derive(Component)]
pub(in crate::animal_behavior) struct BehaviorScriptTransaction;

/// Script effects, applied after the script returns in the order it made them.
#[derive(Default)]
pub(super) struct BehaviorScriptWorldChanges {
    pub(super) poo_levels: Vec<(Entity, f32)>,
    pub(super) incomes: Vec<f64>,
}

#[derive(SystemParam)]
pub(in crate::animal_behavior) struct BehaviorScriptWorldAccess<'w, 's> {
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    definitions: Res<'w, WorldDefinitions>,
    objects: Query<
        'w,
        's,
        (
            Entity,
            &'static DefinitionId,
            &'static GlobalTransform,
            &'static WorldMember,
        ),
    >,
    poo_levels: Query<'w, 's, &'static CompostPooLevel>,
    killed: Query<'w, 's, &'static BehaviorKilledTarget>,
    transactions: MessageWriter<'w, TransactionRequest>,
}

impl BehaviorScriptWorldAccess<'_, '_> {
    /// `BFG_GET_ENTITIES`: live world objects of one authored binder type.
    pub(super) fn entities_of_type(&self, binder_type: &str) -> Vec<Entity> {
        let definition = AssetId::from_key(&canonicalize_source_document_record_key(binder_type));
        self.objects
            .iter()
            .filter(|(_, candidate, _, _)| candidate.0 == definition)
            .map(|(entity, ..)| entity)
            .collect()
    }

    /// The target this execution killed, which scripts may still read.
    fn killed_target(
        &self,
        actor: Entity,
        execution_id: u64,
        entity: Entity,
    ) -> Option<&BehaviorKilledTarget> {
        self.killed
            .get(actor)
            .ok()
            .filter(|killed| killed.execution_id == execution_id && killed.entity == entity)
    }

    pub(super) fn can_resolve(&self, actor: Entity, execution_id: u64, entity: Entity) -> bool {
        self.objects.contains(entity) || self.killed_target(actor, execution_id, entity).is_some()
    }

    pub(super) fn position(
        &self,
        actor: Entity,
        execution_id: u64,
        entity: Entity,
    ) -> Option<Vec3> {
        self.objects
            .get(entity)
            .map(|(_, _, transform, _)| transform.translation())
            .ok()
            .or_else(|| {
                self.killed_target(actor, execution_id, entity)
                    .map(|killed| killed.translation)
            })
    }

    pub(super) fn binder_type(
        &self,
        actor: Entity,
        execution_id: u64,
        entity: Entity,
    ) -> Option<String> {
        let definition = self
            .objects
            .get(entity)
            .map(|(_, definition, _, _)| definition.0)
            .ok()
            .or_else(|| {
                self.killed_target(actor, execution_id, entity)
                    .map(|killed| killed.definition)
            })?;
        self.definitions
            .get(&self.definition_assets)?
            .object_binder_type(definition)
            .map(str::to_owned)
    }

    pub(super) fn poo_level(&self, entity: Entity) -> f32 {
        self.poo_levels.get(entity).map_or(0.0, |level| level.0)
    }

    pub(super) fn apply(
        &mut self,
        commands: &mut Commands,
        actor: Entity,
        changes: BehaviorScriptWorldChanges,
    ) {
        for (entity, level) in changes.poo_levels {
            if let Ok(mut entity) = commands.get_entity(entity) {
                entity.insert(CompostPooLevel(level));
            }
        }
        let world = self.objects.get(actor).map(|(_, _, _, world)| *world).ok();
        for income in changes.incomes {
            let Some(amount) = authored_currency_to_money(income) else {
                warn!(
                    ?actor,
                    income, "behavior script income is not a valid amount"
                );
                continue;
            };
            let mut operation = commands.spawn(BehaviorScriptTransaction);
            if let Some(world) = world {
                operation.insert(world);
            }
            // `ZT_GENERIC_TRANSACTION` "destroy" books an object sale.
            self.transactions.write(TransactionRequest {
                operation: operation.id(),
                debit: Account::External,
                credit: Account::Zoo,
                amount,
                kind: TransactionKind::Refund,
                subject: None,
            });
        }
    }
}

pub(in crate::animal_behavior) fn remove_settled_behavior_script_transactions(
    mut commands: Commands,
    operations: Query<(), With<BehaviorScriptTransaction>>,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
) {
    for operation in completed
        .read()
        .map(|transaction| transaction.operation)
        .chain(rejected.read().map(|transaction| transaction.operation))
    {
        if operations.contains(operation) {
            commands.entity(operation).despawn();
        }
    }
}
