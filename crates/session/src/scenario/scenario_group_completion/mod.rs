use std::collections::BTreeMap;

use bevy::prelude::*;

use openzt2_game_assets::world_scenario::authored_world_scenario_state::AuthoredWorldScenarioState;
use openzt2_game_data::{world_scenario::ScenarioObjectiveGroupRecord, AssetId};
use openzt2_zoo_state::scenario::scenario_objective_types::{
    ScenarioObjectiveGroup, ScenarioObjectiveGroupOf, ScenarioObjectiveStatus,
};

#[derive(Component)]
pub(super) struct ScenarioCompletionGroup;

#[derive(Component)]
pub(super) struct ScenarioRootGroup(pub usize);

#[derive(Component)]
#[relationship(relationship_target = ScenarioGroupChildren)]
pub(super) struct ScenarioGroupChildOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ScenarioGroupChildOf)]
pub(super) struct ScenarioGroupChildren(Vec<Entity>);

pub(super) fn reduce_group(
    all_success: bool,
    statuses: impl IntoIterator<Item = ScenarioObjectiveStatus>,
) -> ScenarioObjectiveStatus {
    use ScenarioObjectiveStatus::{Active, Failed, Satisfied};
    let (deciding, neutral) = if all_success {
        (Failed, Satisfied)
    } else {
        (Satisfied, Failed)
    };
    let mut result = neutral;
    for status in statuses {
        if status == deciding {
            return deciding;
        }
        if status != neutral {
            result = Active;
        }
    }
    result
}

pub(crate) fn spawn_scenario_objective_groups(
    commands: &mut Commands,
    owner: Entity,
    scenario: AssetId,
    groups: &[ScenarioObjectiveGroupRecord],
    mut members: BTreeMap<AssetId, Entity>,
    mut statuses: BTreeMap<AssetId, ScenarioObjectiveStatus>,
    authored: &AuthoredWorldScenarioState,
) {
    let grouped: std::collections::BTreeSet<_> = groups
        .iter()
        .flat_map(|group| group.members.iter().copied())
        .collect();
    for (record_index, group) in groups.iter().enumerate() {
        let status = statuses.get(&group.id).copied().unwrap_or_else(|| {
            if statuses.is_empty() {
                return initial_node_status(authored, group.id)
                    .unwrap_or(ScenarioObjectiveStatus::Active);
            }
            reduce_group(
                group.all_success,
                group
                    .members
                    .iter()
                    .filter(|&&member| !is_optional(authored, member))
                    .map(|member| {
                        statuses
                            .get(member)
                            .copied()
                            .unwrap_or(ScenarioObjectiveStatus::Active)
                    }),
            )
        });
        let group_entity = commands
            .spawn((
                ScenarioObjectiveGroupOf(owner),
                ScenarioObjectiveGroup {
                    record_index,
                    status,
                },
            ))
            .id();
        if !statuses.is_empty() {
            statuses.insert(group.id, status);
        }
        for member in &group.members {
            if let Some(&child) = members.get(member) {
                commands
                    .entity(child)
                    .insert(ScenarioGroupChildOf(group_entity));
            }
        }
        members.insert(group.id, group_entity);
    }
    if let Some(roots) = authored.objective_roots.get(&scenario) {
        for (index, id) in roots.iter().enumerate() {
            if let Some(&entity) = members.get(id) {
                let mut root = commands.entity(entity);
                root.insert(ScenarioRootGroup(index));
                if !is_optional(authored, *id) {
                    root.insert(ScenarioCompletionGroup);
                }
            }
        }
    } else {
        for (index, group) in groups.iter().enumerate() {
            if !grouped.contains(&group.id) {
                let mut root = commands.entity(members[&group.id]);
                root.insert(ScenarioRootGroup(index));
                if !is_optional(authored, group.id) {
                    root.insert(ScenarioCompletionGroup);
                }
            }
        }
    }
}

pub(super) fn is_optional(authored: &AuthoredWorldScenarioState, id: AssetId) -> bool {
    authored
        .scenario_nodes
        .get(&id)
        .is_some_and(|policy| policy.optional == Some(true))
}

pub(super) fn initial_node_status(
    authored: &AuthoredWorldScenarioState,
    id: AssetId,
) -> Option<ScenarioObjectiveStatus> {
    let state = authored.scenario_nodes.get(&id)?.initial_state?;
    match state {
        -1 => Some(ScenarioObjectiveStatus::Failed),
        0 => Some(ScenarioObjectiveStatus::Active),
        1 => Some(ScenarioObjectiveStatus::Satisfied),
        2 => Some(ScenarioObjectiveStatus::Inactive),
        _ => None,
    }
}
