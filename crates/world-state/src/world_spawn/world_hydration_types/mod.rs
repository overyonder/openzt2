use bevy::prelude::{Component, Entity};

#[derive(Component)]
#[relationship(relationship_target = PendingStartingAnimals)]
pub struct StartingAnimalPendingFor(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = StartingAnimalPendingFor)]
pub struct PendingStartingAnimals(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = PendingTerrainProjections)]
pub struct TerrainProjectionPendingFor(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = TerrainProjectionPendingFor)]
pub struct PendingTerrainProjections(Vec<Entity>);

#[derive(Component)]
pub struct WorldHydration {
    record_count: usize,
    committed_count: usize,
}

impl WorldHydration {
    pub fn new_pending_world_hydration(record_count: usize) -> Self {
        Self {
            record_count,
            committed_count: 0,
        }
    }

    pub fn prefab_loading_progress(&self) -> (usize, usize) {
        (self.committed_count, self.record_count)
    }

    pub fn advance_prefab_loading_progress(&mut self) {
        self.committed_count += 1;
    }
}
