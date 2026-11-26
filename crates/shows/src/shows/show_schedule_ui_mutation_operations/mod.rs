use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_world_state::world_spawn::{
    persistent_id_assignment::AssignPersistentId, world_membership_types::WorldMember,
};

use super::show_schedule_types::{
    ScheduledShowPerformancePlan, ScheduledShowRow, ShowScheduleRowOrder, ShowStageRows,
};

#[derive(SystemParam)]
pub(super) struct ShowScheduleRows<'w, 's> {
    stages: Query<'w, 's, Option<&'static ShowStageRows>>,
    rows: Query<
        'w,
        's,
        (
            &'static ScheduledShowRow,
            &'static mut ShowScheduleRowOrder,
            Has<ScheduledShowPerformancePlan>,
        ),
    >,
}

#[derive(Default)]
pub(super) struct ShowScheduleEdits {
    orders: HashMap<Entity, Vec<(u16, Entity)>>,
    added: Vec<(Entity, Option<WorldMember>)>,
    new_rows: HashMap<Entity, (Entity, bool)>,
    deleted: HashSet<Entity>,
}

impl ShowScheduleEdits {
    pub(super) fn row_facts(
        &self,
        row: Entity,
        schedules: &ShowScheduleRows<'_, '_>,
    ) -> Option<(Entity, bool)> {
        if self.deleted.contains(&row) {
            return None;
        }
        self.new_rows.get(&row).copied().or_else(|| {
            schedules
                .rows
                .get(row)
                .ok()
                .map(|(owner, _, show)| (owner.stage, show))
        })
    }

    fn stage_order(
        &mut self,
        stage: Entity,
        schedules: &ShowScheduleRows<'_, '_>,
    ) -> &mut Vec<(u16, Entity)> {
        self.orders.entry(stage).or_insert_with(|| {
            let mut order = schedules
                .stages
                .get(stage)
                .ok()
                .flatten()
                .into_iter()
                .flat_map(RelationshipTarget::iter)
                .filter_map(|row| Some((schedules.rows.get(row).ok()?.1 .0, row)))
                .collect::<Vec<_>>();
            order.sort_unstable();
            order
        })
    }

    pub(super) fn append(
        &mut self,
        stage: Entity,
        member: Option<&WorldMember>,
        show: bool,
        bundle: impl Bundle,
        schedules: &ShowScheduleRows<'_, '_>,
        commands: &mut Commands,
    ) -> Entity {
        let order = self.stage_order(stage, schedules);
        let next = order.last().map_or(0, |(order, _)| order.saturating_add(1));
        let mut entity = commands.spawn((ScheduledShowRow { stage }, bundle));
        if let Some(member) = member {
            entity.insert(*member);
        }
        let row = entity.id();
        order.push((next, row));
        self.new_rows.insert(row, (stage, show));
        self.added.push((row, member.copied()));
        row
    }

    pub(super) fn delete(
        &mut self,
        row: Entity,
        schedules: &ShowScheduleRows<'_, '_>,
        commands: &mut Commands,
    ) {
        let Some((stage, _)) = self.row_facts(row, schedules) else {
            return;
        };
        let order = self.stage_order(stage, schedules);
        let Some(index) = order.iter().position(|(_, entity)| *entity == row) else {
            return;
        };
        let (previous, _) = order.remove(index);
        for (position, _) in &mut order[index..] {
            if *position > previous {
                *position -= 1;
            }
        }
        self.deleted.insert(row);
        commands.entity(row).try_despawn();
    }

    pub(super) fn move_adjacent(
        &mut self,
        row: Entity,
        direction: i16,
        schedules: &ShowScheduleRows<'_, '_>,
    ) {
        let Some((stage, _)) = self.row_facts(row, schedules) else {
            return;
        };
        let order = self.stage_order(stage, schedules);
        let Some(index) = order.iter().position(|(_, entity)| *entity == row) else {
            return;
        };
        let Some(target) = order[index].0.checked_add_signed(direction) else {
            return;
        };
        let Some(other) = order.iter().position(|(order, _)| *order == target) else {
            return;
        };
        let previous = order[index].0;
        let neighbor = order[other].1;
        order[index] = (previous, neighbor);
        order[other] = (target, row);
    }

    pub(super) fn finish(self, schedules: &mut ShowScheduleRows<'_, '_>, commands: &mut Commands) {
        for (order, row) in self.orders.into_values().flatten() {
            if let Ok((_, mut current, _)) = schedules.rows.get_mut(row) {
                current.set_if_neq(ShowScheduleRowOrder(order));
            } else if self.new_rows.contains_key(&row) {
                commands.entity(row).insert(ShowScheduleRowOrder(order));
            }
        }
        for (row, member) in self.added {
            if let Some(member) = member.filter(|_| !self.deleted.contains(&row)) {
                commands.write_message(AssignPersistentId {
                    entity: row,
                    root: member.root,
                });
            }
        }
    }
}
