use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled, query::Allow},
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use crate::habitat::{
    habitat_cell_ownership_changes::HabitatCellOwnershipChanged,
    habitat_topology_cell_queries::convert_finite_world_position_to_topology_cell,
    habitat_types::{Habitat, HabitatIndex},
};
use crate::topology::topology_graph_types::TopologyGrid;

use super::show_stage_types::{ShowStage, ShowStageInHabitat};

type StagePlacement = (
    Entity,
    &'static Transform,
    &'static WorldMember,
    Option<&'static ShowStageInHabitat>,
);
type ChangedStagePlacement = (
    With<ShowStage>,
    Or<(Added<ShowStage>, Changed<Transform>, Changed<WorldMember>)>,
);
type ReplacedHabitatGrid = (
    With<WorldRoot>,
    Or<(Added<HabitatIndex>, Changed<TopologyGrid>)>,
);
type RetiredStagePlacement<'w, 's> = (
    RemovedComponents<'w, 's, ShowStage>,
    RemovedComponents<'w, 's, Transform>,
    RemovedComponents<'w, 's, WorldMember>,
    RemovedComponents<'w, 's, Disabled>,
    RemovedComponents<'w, 's, ShowStageInHabitat>,
    RemovedComponents<'w, 's, HabitatIndex>,
    RemovedComponents<'w, 's, TopologyGrid>,
);

#[derive(bevy::ecs::system::SystemParam)]
pub struct ShowStageHabitatQueries<'w, 's> {
    roots: Query<'w, 's, (&'static HabitatIndex, &'static TopologyGrid), With<WorldRoot>>,
    reconfigured: Query<'w, 's, Entity, ReplacedHabitatGrid>,
    habitats: Query<'w, 's, &'static WorldMember, With<Habitat>>,
    stages: Query<'w, 's, StagePlacement, With<ShowStage>>,
    moved: Query<'w, 's, Entity, ChangedStagePlacement>,
    disabled: Query<'w, 's, Entity, (With<ShowStageInHabitat>, Added<Disabled>, Allow<Disabled>)>,
    links: Query<'w, 's, (), (With<ShowStageInHabitat>, Allow<Disabled>)>,
}

pub fn project_show_stage_habitats(
    mut commands: Commands,
    queries: ShowStageHabitatQueries,
    mut cells: MessageReader<HabitatCellOwnershipChanged>,
    (
        mut retired_stages,
        mut removed_transforms,
        mut removed_members,
        mut enabled,
        mut lost_links,
        mut removed_indexes,
        mut removed_grids,
    ): RetiredStagePlacement,
) {
    for entity in retired_stages
        .read()
        .chain(removed_transforms.read())
        .chain(removed_members.read())
        .chain(queries.disabled.iter())
    {
        if queries.links.contains(entity) && !queries.stages.contains(entity) {
            commands.entity(entity).try_remove::<ShowStageInHabitat>();
        }
    }

    let mut changed: EntityHashSet = queries
        .moved
        .iter()
        .chain(enabled.read())
        .chain(lost_links.read())
        .collect();
    let reconfigured: EntityHashSet = queries
        .reconfigured
        .iter()
        .chain(removed_indexes.read())
        .chain(removed_grids.read())
        .collect();
    let mut changed_coordinates = HashMap::<Entity, HashSet<IVec2>>::default();
    for batch in cells.read() {
        changed_coordinates
            .entry(batch.root)
            .or_default()
            .extend(batch.cells.iter().copied());
    }

    if !reconfigured.is_empty() || !changed_coordinates.is_empty() {
        for (entity, transform, member, _) in &queries.stages {
            if reconfigured.contains(&member.root)
                || changed_coordinates
                    .get(&member.root)
                    .is_some_and(|coordinates| {
                        queries.roots.get(member.root).is_ok_and(|(_, grid)| {
                            stage_cell(transform, grid)
                                .is_some_and(|cell| coordinates.contains(&cell))
                        })
                    })
            {
                changed.insert(entity);
            }
        }
    }
    let mut changed: Vec<_> = changed.into_iter().collect();
    changed.sort_unstable();
    for entity in changed {
        let Ok((_, transform, member, current)) = queries.stages.get(entity) else {
            continue;
        };
        let next = queries
            .roots
            .get(member.root)
            .ok()
            .and_then(|(index, grid)| index.habitat_for_topology_cell(stage_cell(transform, grid)?))
            .filter(|habitat| {
                queries
                    .habitats
                    .get(*habitat)
                    .is_ok_and(|owner| owner.root == member.root)
            });
        if current.map(|link| link.0) != next {
            if let Some(habitat) = next {
                commands.entity(entity).insert(ShowStageInHabitat(habitat));
            } else {
                commands.entity(entity).remove::<ShowStageInHabitat>();
            }
        }
    }
}

fn stage_cell(transform: &Transform, grid: &TopologyGrid) -> Option<IVec2> {
    convert_finite_world_position_to_topology_cell(
        transform.translation.xz() - grid.origin,
        grid.spacing_m,
    )
}
