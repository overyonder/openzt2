use bevy::prelude::Component;
use bevy::prelude::SystemSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldLoadCompletionSet;

/// Durable completion fact for systems whose assets or resources may become
/// available after the one-frame world-load completion notification.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct WorldLoadCompleted;

#[derive(Component)]
pub struct WorldClockInitialized;

#[derive(Component)]
pub struct ImportedPersistentIdsReserved;

#[derive(Component)]
pub struct StartingTerrainHydrated;

#[derive(Component)]
pub struct StartingFenceTopologyHydrated;

#[derive(Component)]
pub struct StartingPathsHydrated;

#[derive(Component)]
pub struct StartingPrefabsHydrated;

#[derive(Component)]
pub struct StartingSpawnValuesApplied;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorldLoadPreparationSet {
    Accept,
    InitializeClock,
    HydrateTerrain,
}
