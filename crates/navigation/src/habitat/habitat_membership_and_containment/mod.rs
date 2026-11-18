use bevy::{
    ecs::{
        entity::EntityHashSet, entity_disabling::Disabled, query::Allow,
        relationship::RelationshipTarget, world::EntityWorldMut,
    },
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::habitat::{
    habitat_topology_cell_queries::convert_finite_world_position_to_topology_cell,
    habitat_types::{
        Containment, ContainmentChanged, HabitatChanged, HabitatIndex, HabitatLocatable,
        HabitatMember, HabitatMembers, HabitatSummary,
    },
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

#[derive(Resource, Default)]
pub(super) struct HabitatLocationCells(HashMap<IVec2, Entity>);

#[derive(Component)]
pub(super) struct HabitatLocationCell(IVec2);

#[derive(Component)]
#[relationship(relationship_target = HabitatCellOccupants)]
pub(super) struct HabitatLocationOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = HabitatLocationOf)]
pub(super) struct HabitatCellOccupants(Vec<Entity>);

#[derive(Message)]
pub(super) struct LostHabitatMembership(Entity);

pub(super) fn retire_empty_habitat_location_cell(
    removed: On<Remove<HabitatCellOccupants>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .queue_silenced(|entity: EntityWorldMut| {
            if !entity.contains::<HabitatCellOccupants>() {
                entity.despawn();
            }
        });
}

pub(super) fn discard_habitat_location_cell(
    discarded: On<Discard<HabitatLocationCell>>,
    cells: Query<&HabitatLocationCell, Allow<Disabled>>,
    mut locations: ResMut<HabitatLocationCells>,
) {
    if let Ok(cell) = cells.get(discarded.entity) {
        if locations.0.get(&cell.0) == Some(&discarded.entity) {
            locations.0.remove(&cell.0);
        }
    }
}

pub(super) fn release_members_of_retired_habitat(
    removed: On<Remove<HabitatMember>>,
    mut members: Query<(&HabitatMember, &mut Containment), Allow<Disabled>>,
    habitats: Query<(), With<HabitatSummary>>,
    mut lost: MessageWriter<LostHabitatMembership>,
) {
    let Ok((membership, mut containment)) = members.get_mut(removed.entity) else {
        return;
    };
    if !habitats.contains(membership.habitat_entity) {
        containment.set_if_neq(Containment::default());
        lost.write(LostHabitatMembership(removed.entity));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn resolve_changed_habitat_membership_and_containment(
    mut commands: Commands,
    habitat_index: Res<HabitatIndex>,
    grid: Res<TopologyGrid>,
    roots: Query<Entity, With<WorldRoot>>,
    mut locations: ResMut<HabitatLocationCells>,
    cell_occupants: Query<&HabitatCellOccupants>,
    habitat_members: Query<&HabitatMembers>,
    habitat_summaries: Query<&HabitatSummary>,
    mut habitat_changes: MessageReader<HabitatChanged>,
    mut lost: MessageReader<LostHabitatMembership>,
    changed_locatables: Query<
        Entity,
        (
            With<HabitatLocatable>,
            Or<(
                Changed<GlobalTransform>,
                Added<HabitatLocatable>,
                Added<HabitatMember>,
            )>,
        ),
    >,
    mut locatables: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&HabitatLocationOf>,
            Option<&HabitatMember>,
            &mut Containment,
        ),
        With<HabitatLocatable>,
    >,
    mut notifications: MessageWriter<ContainmentChanged>,
) {
    let mut affected: EntityHashSet = changed_locatables.iter().collect();
    let lost: EntityHashSet = lost.read().map(|lost| lost.0).collect();
    affected.extend(lost.iter().copied());
    if grid.is_changed() {
        affected.extend(locatables.iter().map(|(entity, ..)| entity));
    }
    let mut changed_habitats = EntityHashSet::default();
    let bounds = habitat_changes
        .read()
        .map(|change| {
            if changed_habitats.insert(change.habitat_entity) {
                if let Ok(members) = habitat_members.get(change.habitat_entity) {
                    affected.extend(members.iter());
                }
            }
            change.changed_topology_bounds
        })
        .reduce(|current, next| current.union(next));
    if let Some(bounds) = bounds {
        for y in bounds.min.y..=bounds.max.y {
            for x in bounds.min.x..=bounds.max.x {
                if let Some(occupants) = locations
                    .0
                    .get(&IVec2::new(x, y))
                    .and_then(|&cell| cell_occupants.get(cell).ok())
                {
                    affected.extend(occupants.iter());
                }
            }
        }
    }
    let mut affected: Vec<Entity> = affected.into_iter().collect();
    affected.sort_unstable();
    for entity in affected {
        let Ok((_, transform, old_location, old_habitat, mut containment)) =
            locatables.get_mut(entity)
        else {
            continue;
        };
        let cell = convert_finite_world_position_to_topology_cell(
            transform.translation().xz() - grid.origin,
            grid.spacing_m,
        );
        let location = cell.map(|coordinate| {
            *locations.0.entry(coordinate).or_insert_with(|| {
                let mut cell = commands.spawn(HabitatLocationCell(coordinate));
                if let Ok(root) = roots.single() {
                    cell.insert(ChildOf(root));
                }
                cell.id()
            })
        });
        if old_location.map(|location| location.0) != location {
            if let Some(location) = location {
                commands.entity(entity).insert(HabitatLocationOf(location));
            } else {
                commands.entity(entity).remove::<HabitatLocationOf>();
            }
        }
        let habitat = cell.and_then(|cell| habitat_index.habitat_for_topology_cell(cell));
        let previous = old_habitat.map(|membership| membership.habitat_entity);
        let is_contained = habitat
            .and_then(|habitat| habitat_summaries.get(habitat).ok())
            .is_some_and(|summary| !summary.boundary_is_breached);
        if previous != habitat {
            if let Some(habitat_entity) = habitat {
                commands
                    .entity(entity)
                    .insert(HabitatMember { habitat_entity });
            } else {
                commands.entity(entity).remove::<HabitatMember>();
            }
        }
        if containment.set_if_neq(Containment { is_contained })
            || previous != habitat
            || previous.is_some_and(|habitat| changed_habitats.contains(&habitat))
            || lost.contains(&entity)
        {
            notifications.write(ContainmentChanged {
                affected_entity: entity,
                habitat_entity: habitat,
                is_contained,
            });
        }
    }
}
