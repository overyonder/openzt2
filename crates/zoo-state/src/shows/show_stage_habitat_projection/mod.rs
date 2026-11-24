use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use crate::habitat::{
    habitat_membership_and_containment::locate_habitat_at_world_position,
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
type ChangedHabitatTopology = (
    With<WorldRoot>,
    Or<(Changed<HabitatIndex>, Changed<TopologyGrid>)>,
);

pub fn project_show_stage_habitats(
    mut commands: Commands,
    roots: Query<(&HabitatIndex, &TopologyGrid), With<WorldRoot>>,
    changed_roots: Query<(), ChangedHabitatTopology>,
    habitats: Query<&WorldMember, With<Habitat>>,
    stages: Query<StagePlacement, With<ShowStage>>,
    changed_stages: Query<Entity, ChangedStagePlacement>,
    (mut removed_stages, mut removed_transforms, mut removed_members): (
        RemovedComponents<ShowStage>,
        RemovedComponents<Transform>,
        RemovedComponents<WorldMember>,
    ),
) {
    for entity in removed_stages
        .read()
        .chain(removed_transforms.read())
        .chain(removed_members.read())
    {
        if !stages.contains(entity) {
            if let Ok(mut entity) = commands.get_entity(entity) {
                entity.try_remove::<ShowStageInHabitat>();
            }
        }
    }

    let mut project = |(entity, transform, member, current): (
        Entity,
        &Transform,
        &WorldMember,
        Option<&ShowStageInHabitat>,
    )| {
        let next = roots.get(member.root).ok().and_then(|(index, grid)| {
            locate_habitat_at_world_position(index, transform.translation.xz(), *grid).filter(
                |habitat| {
                    habitats
                        .get(*habitat)
                        .is_ok_and(|owner| owner.root == member.root)
                },
            )
        });
        if current.map(|link| link.0) != next {
            let mut entity = commands.entity(entity);
            if let Some(habitat) = next {
                entity.insert(ShowStageInHabitat(habitat));
            } else {
                entity.remove::<ShowStageInHabitat>();
            }
        }
    };
    if changed_roots.is_empty() {
        for entity in &changed_stages {
            if let Ok(stage) = stages.get(entity) {
                project(stage);
            }
        }
    } else {
        for stage in &stages {
            if changed_roots.contains(stage.2.root) || changed_stages.contains(stage.0) {
                project(stage);
            }
        }
    }
}
