use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::construction::{
    construction_edit_history_types::{
        ConstructionEditDirection, ConstructionTransactionApplied,
        RecordAppliedConstructionTransaction,
    },
    construction_transaction_types::{EditTransaction, TransactionState},
};

#[derive(Component)]
#[relationship(relationship_target = ConstructionHistoryEntries)]
pub(super) struct HistoryOf(Entity);

/// Retained construction transactions, in settlement order, on the world entity.
#[derive(Component)]
#[relationship_target(relationship = HistoryOf, linked_spawn)]
pub(super) struct ConstructionHistoryEntries(Vec<Entity>);

/// Sequence allocation and retention for edits belonging to this world.
#[derive(Component, Debug)]
pub struct WorldConstructionHistory {
    pub limit: usize,
    next_sequence: u64,
}

impl Default for WorldConstructionHistory {
    fn default() -> Self {
        Self {
            limit: 128,
            next_sequence: 1,
        }
    }
}

impl WorldConstructionHistory {
    pub fn allocate_sequence(&mut self) -> Option<u64> {
        let sequence = self.next_sequence;
        self.next_sequence = sequence.checked_add(1)?;
        Some(sequence)
    }
}

#[derive(SystemParam)]
pub struct ConstructionHistory<'w, 's> {
    worlds: Query<'w, 's, &'static ConstructionHistoryEntries>,
    edits: Query<'w, 's, &'static EditTransaction>,
}

impl ConstructionHistory<'_, '_> {
    pub fn next(&self, direction: ConstructionEditDirection) -> Option<Entity> {
        let entries = self.worlds.single().ok()?;
        match direction {
            ConstructionEditDirection::Undo => entries.iter().rev().find(|&entity| {
                self.edits
                    .get(entity)
                    .is_ok_and(|edit| edit.state == TransactionState::Applied)
            }),
            ConstructionEditDirection::Redo => entries.iter().find(|&entity| {
                self.edits
                    .get(entity)
                    .is_ok_and(|edit| edit.state == TransactionState::Undone)
            }),
        }
    }
}

pub(super) fn retain_accepted_edits(
    mut commands: Commands,
    mut records: MessageReader<RecordAppliedConstructionTransaction>,
) {
    for request in records.read() {
        // Each retained edit must see earlier inserts and evictions from this tick.
        commands.run_system_cached_with(retain_world_edit, request.0);
    }
}

fn retain_world_edit(
    In(entity): In<Entity>,
    mut commands: Commands,
    worlds: Query<(
        &WorldConstructionHistory,
        Option<&ConstructionHistoryEntries>,
    )>,
    edits: Query<(&EditTransaction, &WorldMember), Without<HistoryOf>>,
    retained: Query<&EditTransaction, With<HistoryOf>>,
) {
    let Ok((edit, member)) = edits.get(entity) else {
        return;
    };
    if edit.state != TransactionState::Applied {
        return;
    }
    let Ok((history, entries)) = worlds.get(member.root) else {
        return;
    };
    if let Some(entries) = entries {
        let mut applied = Vec::new();
        for retained_entity in entries.iter() {
            match retained.get(retained_entity) {
                Ok(edit) if edit.state == TransactionState::Undone => {
                    commands.entity(retained_entity).try_despawn();
                }
                Ok(_) => applied.push(retained_entity),
                Err(_) => {}
            }
        }
        let overflow = applied
            .len()
            .saturating_add(1)
            .saturating_sub(history.limit);
        for old in applied.into_iter().take(overflow) {
            commands.entity(old).try_despawn();
        }
    }
    if history.limit == 0 {
        commands.entity(entity).try_despawn();
    } else {
        commands.entity(entity).insert(HistoryOf(member.root));
    }
}

pub(super) fn record_history_application(
    mut results: MessageReader<ConstructionTransactionApplied>,
    mut edits: Query<&mut EditTransaction, With<HistoryOf>>,
) {
    for result in results.read().filter(|result| result.accepted) {
        if let Ok(mut edit) = edits.get_mut(result.transaction) {
            edit.state = match result.direction {
                ConstructionEditDirection::Undo => TransactionState::Undone,
                ConstructionEditDirection::Redo => TransactionState::Applied,
            };
        }
    }
}
