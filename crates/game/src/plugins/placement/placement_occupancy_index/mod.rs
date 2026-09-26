use bevy::{platform::collections::HashMap, prelude::*};

/// Derived acceleration for occupied footprint cells. Placed-object components
/// remain authoritative and reconstruct this resource after loading.
#[derive(Resource, Debug, Default)]
pub(crate) struct PlacedObjectFootprintOccupancyIndex {
    occupied_cells: HashMap<IVec2, PlacedObjectCellOccupants>,
}

#[derive(Debug)]
enum PlacedObjectCellOccupants {
    One(Entity),
    Many(Vec<Entity>),
}

impl PlacedObjectCellOccupants {
    fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        let (first, rest) = match self {
            Self::One(entity) => (Some(*entity), &[][..]),
            Self::Many(entities) => (entities.first().copied(), &entities[1..]),
        };
        first.into_iter().chain(rest.iter().copied())
    }

    fn insert(&mut self, entity: Entity) {
        match self {
            Self::One(existing) if *existing != entity => {
                *self = Self::Many(vec![*existing, entity]);
            }
            Self::Many(entities) if !entities.contains(&entity) => entities.push(entity),
            Self::One(_) | Self::Many(_) => {}
        }
    }

    fn remove(&mut self, entity: Entity) -> bool {
        match self {
            Self::One(existing) => *existing == entity,
            Self::Many(entities) => {
                entities.retain(|occupant| *occupant != entity);
                if let [remaining] = entities.as_slice() {
                    *self = Self::One(*remaining);
                }
                false
            }
        }
    }
}

impl PlacedObjectFootprintOccupancyIndex {
    pub(super) fn clear(&mut self) {
        self.occupied_cells.clear();
    }

    pub(super) fn reserve(&mut self, additional: usize) {
        self.occupied_cells.reserve(additional);
    }

    pub(super) fn entities_occupying_cell(&self, cell: IVec2) -> impl Iterator<Item = Entity> + '_ {
        self.occupied_cells
            .get(&cell)
            .into_iter()
            .flat_map(PlacedObjectCellOccupants::entities)
    }

    pub(super) fn cell_contains_entity(&self, cell: IVec2, entity: Entity) -> bool {
        self.occupied_cells
            .get(&cell)
            .is_some_and(|entities| entities.entities().any(|occupant| occupant == entity))
    }

    pub(super) fn insert_entity_into_cell(&mut self, cell: IVec2, entity: Entity) {
        match self.occupied_cells.entry(cell) {
            bevy::platform::collections::hash_map::Entry::Occupied(mut occupants) => {
                occupants.get_mut().insert(entity);
            }
            bevy::platform::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(PlacedObjectCellOccupants::One(entity));
            }
        }
    }

    pub(super) fn remove_entity_from_cell(&mut self, cell: IVec2, entity: Entity) {
        if self
            .occupied_cells
            .get_mut(&cell)
            .is_some_and(|occupants| occupants.remove(entity))
        {
            self.occupied_cells.remove(&cell);
        }
    }
}
