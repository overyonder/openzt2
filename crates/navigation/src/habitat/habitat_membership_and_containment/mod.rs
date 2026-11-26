use bevy::{
    ecs::{
        entity::EntityHashSet, entity_disabling::Disabled, query::Allow,
        relationship::RelationshipTarget, world::EntityWorldMut,
    },
    prelude::*,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::habitat::{
    habitat_cell_ownership_changes::HabitatCellOwnershipChanged,
    habitat_topology_cell_queries::convert_finite_world_position_to_topology_cell,
    habitat_types::{
        Containment, ContainmentChanged, HabitatCellOf, HabitatChanged, HabitatGridCell,
        HabitatIndex, HabitatLocatable, HabitatMember, HabitatMembers, HabitatSummary,
    },
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

#[derive(Component)]
#[relationship(relationship_target = HabitatCellOccupants)]
pub(super) struct HabitatLocationOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = HabitatLocationOf)]
pub(super) struct HabitatCellOccupants(Vec<Entity>);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HabitatCellSectors(pub(super) [Option<Entity>; 4]);

impl HabitatCellSectors {
    pub(super) fn habitat_at(
        &self,
        position: Vec2,
        tile: IVec2,
        grid: TopologyGrid,
    ) -> Option<Entity> {
        let centre = grid.origin + (tile.as_vec2() + Vec2::splat(0.5)) * grid.spacing_m;
        let offset = position - centre;
        let side = super::placement_sectors::sector_for_offset(Vec2::new(offset.x, -offset.y));
        self.0[usize::from(side / 2)]
    }
}

#[derive(Message)]
pub(super) struct LostHabitatMembership(Entity);

pub(super) fn retire_empty_habitat_location_cell<C: Component>(
    removed: On<Remove<C>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .queue_silenced(|entity: EntityWorldMut| {
            if !entity.contains::<HabitatCellOccupants>()
                && !entity.contains::<HabitatCellOf>()
                && !entity.contains::<HabitatCellSectors>()
            {
                entity.despawn();
            }
        });
}

pub(super) fn discard_habitat_location_cell(
    discarded: On<Discard<HabitatGridCell>>,
    cells: Query<(&HabitatGridCell, &ChildOf), Allow<Disabled>>,
    mut indexes: Query<&mut HabitatIndex>,
) {
    if let Ok((cell, root)) = cells.get(discarded.entity) {
        if let Ok(mut index) = indexes.get_mut(root.parent()) {
            index.remove_cell(cell.0, discarded.entity);
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
    roots: Single<(Entity, &mut HabitatIndex, Ref<TopologyGrid>), With<WorldRoot>>,
    cell_occupants: Query<&HabitatCellOccupants>,
    cell_owners: Query<&HabitatCellOf>,
    partitioned: Query<&HabitatCellSectors>,
    habitat_members: Query<&HabitatMembers>,
    habitat_summaries: Query<&HabitatSummary>,
    mut habitat_changes: MessageReader<HabitatChanged>,
    mut cell_changes: MessageReader<HabitatCellOwnershipChanged>,
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
    let (root, mut habitat_index, grid) = roots.into_inner();
    let mut affected: EntityHashSet = changed_locatables.iter().collect();
    let lost: EntityHashSet = lost.read().map(|lost| lost.0).collect();
    affected.extend(lost.iter().copied());
    if grid.is_changed() {
        affected.extend(locatables.iter().map(|(entity, ..)| entity));
    }
    let mut changed_habitats = EntityHashSet::default();
    for change in habitat_changes.read() {
        if changed_habitats.insert(change.habitat_entity) {
            if let Ok(members) = habitat_members.get(change.habitat_entity) {
                affected.extend(members.iter());
            }
        }
    }
    for change in cell_changes.read().filter(|change| change.root == root) {
        for coordinate in &change.cells {
            if let Some(occupants) = habitat_index
                .cell_at(*coordinate)
                .and_then(|cell| cell_occupants.get(cell).ok())
            {
                affected.extend(occupants.iter());
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
            habitat_index.cell_at(coordinate).unwrap_or_else(|| {
                let cell = commands
                    .spawn((HabitatGridCell(coordinate), ChildOf(root)))
                    .id();
                habitat_index.record_cell(coordinate, cell);
                cell
            })
        });
        if old_location.map(|location| location.0) != location {
            if let Some(location) = location {
                commands.entity(entity).insert(HabitatLocationOf(location));
            } else {
                commands.entity(entity).remove::<HabitatLocationOf>();
            }
        }
        let habitat =
            location
                .zip(cell)
                .and_then(|(location, tile)| match partitioned.get(location) {
                    Ok(sectors) => sectors.habitat_at(transform.translation().xz(), tile, *grid),
                    Err(_) => cell_owners.get(location).ok().map(|owner| owner.0),
                });
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
