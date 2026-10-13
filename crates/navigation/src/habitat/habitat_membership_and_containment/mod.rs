use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::Containment;
use openzt2_zoo_state::habitat::habitat_types::ContainmentChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::HabitatLocatable;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

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
