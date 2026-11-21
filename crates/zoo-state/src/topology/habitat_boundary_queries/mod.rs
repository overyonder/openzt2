use bevy::{ecs::system::SystemParam, prelude::*};

use crate::habitat::habitat_types::{HabitatBoundaryFence, HabitatFenceIncidences};

#[derive(SystemParam)]
pub struct HabitatBoundaryQueries<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    fences: Query<'w, 's, &'static HabitatBoundaryFence>,
    incidences: Query<'w, 's, &'static HabitatFenceIncidences>,
    owners: Query<'w, 's, &'static ChildOf>,
}

impl HabitatBoundaryQueries<'_, '_> {
    pub fn fences(&self, habitat: Entity) -> impl Iterator<Item = Entity> + '_ {
        self.children
            .get(habitat)
            .ok()
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| self.fences.get(child).ok().map(|fence| fence.0))
    }

    pub fn habitats(&self, fence: Entity) -> impl Iterator<Item = Entity> + '_ {
        self.incidences
            .get(fence)
            .ok()
            .into_iter()
            .flat_map(|incidences| incidences.iter())
            .filter_map(|incidence| self.owners.get(incidence).ok().map(ChildOf::parent))
    }
}
