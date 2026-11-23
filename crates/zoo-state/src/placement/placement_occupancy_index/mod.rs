use bevy::{
    ecs::{lifecycle::HookContext, relationship::RelationshipTarget, world::DeferredWorld},
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::placed_object_types::IndexedObjectPlacementCells;

#[derive(Component, Debug, Default)]
pub struct PlacedObjectFootprintOccupancyIndex {
    cells: HashMap<IVec2, Entity>,
}

#[derive(Component)]
#[relationship(relationship_target = FootprintCellOccupants)]
struct OccupiesFootprintCell(Entity);

#[derive(Component)]
#[relationship_target(relationship = OccupiesFootprintCell, linked_spawn)]
struct FootprintCellOccupants(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = PlacedObjectFootprintLinks)]
struct FootprintOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = FootprintOf, linked_spawn)]
struct PlacedObjectFootprintLinks(Vec<Entity>);

#[derive(bevy::ecs::system::SystemParam)]
pub struct PlacedObjectFootprintQueries<'w, 's> {
    indexes: Query<'w, 's, Ref<'static, PlacedObjectFootprintOccupancyIndex>>,
    occupants: Query<'w, 's, &'static FootprintCellOccupants>,
    objects: Query<'w, 's, &'static FootprintOf>,
}

impl PlacedObjectFootprintQueries<'_, '_> {
    pub fn entities_occupying_cell(
        &self,
        root: Entity,
        coordinate: IVec2,
    ) -> impl Iterator<Item = Entity> + '_ {
        let occupants = self
            .indexes
            .get(root)
            .ok()
            .and_then(|index| index.into_inner().cells.get(&coordinate))
            .and_then(|cell| self.occupants.get(*cell).ok());
        occupants.into_iter().flat_map(|occupants| {
            self.objects
                .iter_many(occupants.iter())
                .filter_map(Result::ok)
                .map(|link| link.0)
        })
    }

    pub fn cell_contains_entity(&self, root: Entity, coordinate: IVec2, object: Entity) -> bool {
        self.entities_occupying_cell(root, coordinate)
            .any(|occupant| occupant == object)
    }

    pub fn index_changed(&self, root: Entity) -> bool {
        self.indexes.get(root).is_ok_and(|index| index.is_changed())
    }
}

pub(super) fn register_occupied_cells(mut world: DeferredWorld, context: HookContext) {
    let object = context.entity;
    let Some(member) = world.get::<WorldMember>(object).copied() else {
        return;
    };
    let Some(cells) = world
        .get::<IndexedObjectPlacementCells>(object)
        .filter(|cells| !cells.0.is_empty())
        .map(|cells| cells.0.clone())
    else {
        return;
    };
    world.commands().queue(move |world: &mut World| {
        if world.get::<WorldMember>(object) != Some(&member)
            || world
                .get::<PlacedObjectFootprintOccupancyIndex>(member.root)
                .is_none()
        {
            return;
        }
        for coordinate in cells {
            let cell = match world
                .get::<PlacedObjectFootprintOccupancyIndex>(member.root)
                .and_then(|index| index.cells.get(&coordinate))
                .copied()
            {
                Some(cell) => cell,
                None => {
                    let cell = world.spawn(ChildOf(member.root)).id();
                    if let Some(mut index) =
                        world.get_mut::<PlacedObjectFootprintOccupancyIndex>(member.root)
                    {
                        index.cells.insert(coordinate, cell);
                    }
                    cell
                }
            };
            world.spawn((FootprintOf(object), OccupiesFootprintCell(cell)));
        }
        if let Some(mut index) = world.get_mut::<PlacedObjectFootprintOccupancyIndex>(member.root) {
            index.set_changed();
        }
    });
}

pub(super) fn discard_occupied_cells(mut world: DeferredWorld, context: HookContext) {
    let object = context.entity;
    let Some(member) = world.get::<WorldMember>(object).copied() else {
        return;
    };
    let Some(cells) = world
        .get::<IndexedObjectPlacementCells>(object)
        .filter(|cells| !cells.0.is_empty())
        .map(|cells| cells.0.clone())
    else {
        return;
    };
    world.commands().queue(move |world: &mut World| {
        if let Ok(mut object) = world.get_entity_mut(object) {
            object.despawn_related::<PlacedObjectFootprintLinks>();
        }
        for coordinate in cells {
            let cell = world
                .get::<PlacedObjectFootprintOccupancyIndex>(member.root)
                .and_then(|index| index.cells.get(&coordinate))
                .copied();
            let Some(cell) = cell else {
                continue;
            };
            if world
                .get::<FootprintCellOccupants>(cell)
                .is_none_or(RelationshipTarget::is_empty)
            {
                if let Some(mut index) =
                    world.get_mut::<PlacedObjectFootprintOccupancyIndex>(member.root)
                {
                    index.cells.remove(&coordinate);
                }
                let _ = world.despawn(cell);
            }
        }
        if let Some(mut index) = world.get_mut::<PlacedObjectFootprintOccupancyIndex>(member.root) {
            index.set_changed();
        }
    });
}
