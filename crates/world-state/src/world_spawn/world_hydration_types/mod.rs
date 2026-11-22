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
    spawned_roots: Vec<Entity>,
}

impl WorldHydration {
    pub fn new_pending_world_hydration(record_count: usize) -> Self {
        Self {
            record_count,
            spawned_roots: Vec::with_capacity(record_count),
        }
    }

    pub fn prefab_loading_progress(&self) -> (usize, usize) {
        (self.spawned_roots.len(), self.record_count)
    }

    pub fn spawned_world_prefab_root_at_record_index(&self, index: u32) -> Option<Entity> {
        self.spawned_roots.get(index as usize).copied()
    }

    pub fn record_world_prefab_spawn_completion(&mut self, entity: Entity) {
        self.spawned_roots.push(entity);
    }
}
