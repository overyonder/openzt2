use bevy::{platform::collections::HashSet, prelude::*};

use crate::plugins::topology::topology_graph_types::TopologyGrid;

use super::{
    habitat_topology_cell_queries::convert_finite_world_position_to_topology_cell,
    habitat_types::{
        Containment, ContainmentChanged, HabitatChanged, HabitatIndex, HabitatLocatable,
        HabitatMember, HabitatSummary,
    },
};

/// Locates the habitat containing one finite world-XZ position.
pub(crate) fn locate_habitat_at_world_position(
    habitat_index: &HabitatIndex,
    world_position_xz: Vec2,
    topology_grid: TopologyGrid,
) -> Option<Entity> {
    convert_finite_world_position_to_topology_cell(
        world_position_xz - topology_grid.origin,
        topology_grid.spacing_m,
    )
    .and_then(|topology_cell| habitat_index.habitat_for_topology_cell(topology_cell))
}

pub(super) fn update_habitat_membership_after_locatable_entities_move(
    mut commands: Commands,
    habitat_index: Res<HabitatIndex>,
    locatable_entities: Query<
        (Entity, &GlobalTransform, Option<&HabitatMember>),
        (
            With<HabitatLocatable>,
            Or<(
                Changed<GlobalTransform>,
                Without<HabitatMember>,
                Without<Containment>,
            )>,
        ),
    >,
    topology_grid: Res<TopologyGrid>,
    habitat_summaries: Query<&HabitatSummary>,
    mut containment_components: Query<&mut Containment>,
    mut containment_changes: MessageWriter<ContainmentChanged>,
) {
    for (entity, transform, previous_membership) in &locatable_entities {
        let translation = transform.translation();
        let habitat = locate_habitat_at_world_position(
            &habitat_index,
            Vec2::new(translation.x, translation.z),
            *topology_grid,
        );
        if previous_membership.map(|membership| membership.habitat_entity) == habitat
            && (habitat.is_none() || containment_components.contains(entity))
        {
            continue;
        }
        let is_contained = habitat
            .and_then(|habitat| habitat_summaries.get(habitat).ok())
            .is_some_and(|summary| !summary.boundary_is_breached);
        match habitat {
            Some(habitat_entity) => {
                commands
                    .entity(entity)
                    .insert(HabitatMember { habitat_entity });
                if let Ok(mut current_containment) = containment_components.get_mut(entity) {
                    *current_containment = Containment {
                        habitat_entity,
                        is_contained,
                    };
                } else {
                    commands.entity(entity).insert(Containment {
                        habitat_entity,
                        is_contained,
                    });
                }
            }
            None => {
                commands
                    .entity(entity)
                    .remove::<(HabitatMember, Containment)>();
            }
        }
        containment_changes.write(ContainmentChanged {
            affected_entity: entity,
            habitat_entity: habitat,
            is_contained,
        });
    }
}

pub(super) fn update_containment_after_habitat_regions_change(
    mut commands: Commands,
    mut habitat_changes: MessageReader<HabitatChanged>,
    habitat_index: Res<HabitatIndex>,
    topology_grid: Res<TopologyGrid>,
    habitat_summaries: Query<&HabitatSummary>,
    mut habitat_members: Query<
        (
            Entity,
            &GlobalTransform,
            &mut HabitatMember,
            Option<&mut Containment>,
        ),
        With<HabitatLocatable>,
    >,
    mut containment_changes: MessageWriter<ContainmentChanged>,
) {
    let changed_habitats: HashSet<_> = habitat_changes
        .read()
        .map(|habitat_change| habitat_change.habitat_entity)
        .collect();
    if changed_habitats.is_empty() {
        return;
    }
    for (entity, transform, mut membership, containment) in &mut habitat_members {
        if !changed_habitats.contains(&membership.habitat_entity) {
            continue;
        }
        let translation = transform.translation();
        let habitat = locate_habitat_at_world_position(
            &habitat_index,
            Vec2::new(translation.x, translation.z),
            *topology_grid,
        );
        let is_contained = habitat
            .and_then(|habitat| habitat_summaries.get(habitat).ok())
            .is_some_and(|summary| !summary.boundary_is_breached);
        match habitat {
            Some(habitat_entity) => {
                membership.habitat_entity = habitat_entity;
                if let Some(mut containment) = containment {
                    containment.habitat_entity = habitat_entity;
                    containment.is_contained = is_contained;
                } else {
                    commands.entity(entity).insert(Containment {
                        habitat_entity,
                        is_contained,
                    });
                }
            }
            None => {
                commands
                    .entity(entity)
                    .remove::<(HabitatMember, Containment)>();
            }
        }
        containment_changes.write(ContainmentChanged {
            affected_entity: entity,
            habitat_entity: habitat,
            is_contained,
        });
    }
}
