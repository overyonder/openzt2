use std::time::{Duration, Instant};

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::asset_source::{AssetArchives, SceneLoadingAssetReadPerformanceSnapshot};

#[derive(Debug, Clone, Copy)]
pub(super) enum WorldLoadingPerformanceStage {
    RequestAcceptance,
    TerrainHydration,
    PersistentIdReservation,
    StartingFenceHydration,
    StartingPathHydration,
    WorldPrefabRecordHydration,
    AuthoredSpawnValueApplication,
    PrefabPresentationHydration,
    CompletionCheck,
}

impl WorldLoadingPerformanceStage {
    const ALL: [Self; 9] = [
        Self::RequestAcceptance,
        Self::TerrainHydration,
        Self::PersistentIdReservation,
        Self::StartingFenceHydration,
        Self::StartingPathHydration,
        Self::WorldPrefabRecordHydration,
        Self::AuthoredSpawnValueApplication,
        Self::PrefabPresentationHydration,
        Self::CompletionCheck,
    ];

    const fn index(self) -> usize {
        self as usize
    }

    const fn name(self) -> &'static str {
        match self {
            Self::RequestAcceptance => "request_acceptance",
            Self::TerrainHydration => "terrain_hydration",
            Self::PersistentIdReservation => "persistent_id_reservation",
            Self::StartingFenceHydration => "starting_fence_hydration",
            Self::StartingPathHydration => "starting_path_hydration",
            Self::WorldPrefabRecordHydration => "world_prefab_record_hydration",
            Self::AuthoredSpawnValueApplication => "authored_spawn_value_application",
            Self::PrefabPresentationHydration => "prefab_presentation_hydration",
            Self::CompletionCheck => "completion_check",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct WorldLoadingPerformanceStageAttribution {
    calls: u64,
    elapsed: Duration,
    maximum: Duration,
}

#[derive(Resource)]
pub(super) struct WorldLoadingPerformanceAttribution {
    enabled: bool,
    active: bool,
    scenario: AssetId,
    started_at: Instant,
    next_progress_report_at: Instant,
    stages: [WorldLoadingPerformanceStageAttribution; 9],
}

impl Default for WorldLoadingPerformanceAttribution {
    fn default() -> Self {
        Self {
            enabled: std::env::var_os("OPENZT2_PROFILE_SCENE_LOADING").is_some(),
            active: false,
            scenario: AssetId::default(),
            started_at: Instant::now(),
            next_progress_report_at: Instant::now(),
            stages: [WorldLoadingPerformanceStageAttribution::default(); 9],
        }
    }
}

impl WorldLoadingPerformanceAttribution {
    pub(super) fn begin(&mut self, scenario: AssetId, archives: &AssetArchives) {
        if !self.enabled || self.active {
            return;
        }
        self.active = true;
        self.scenario = scenario;
        self.started_at = Instant::now();
        self.next_progress_report_at = self.started_at + Duration::from_secs(5);
        self.stages
            .fill(WorldLoadingPerformanceStageAttribution::default());
        archives.begin_scene_loading_asset_read_performance_attribution();
        info!(
            target: "openzt2_scene_loading",
            ?scenario,
            "scene loading attribution started"
        );
    }

    pub(super) fn progress_report_is_due(&mut self) -> bool {
        if !self.active || Instant::now() < self.next_progress_report_at {
            return false;
        }
        self.next_progress_report_at = Instant::now() + Duration::from_secs(5);
        info!(
            target: "openzt2_scene_loading",
            scenario = ?self.scenario,
            wall_ms = self.started_at.elapsed().as_secs_f64() * 1_000.0,
            "scene loading remains pending"
        );
        true
    }

    pub(super) fn measure(
        &mut self,
        stage: WorldLoadingPerformanceStage,
    ) -> WorldLoadingPerformanceStageTimer<'_> {
        let active = self.active;
        WorldLoadingPerformanceStageTimer {
            attribution: active.then_some(self),
            stage,
            started_at: Instant::now(),
        }
    }

    pub(super) fn finish_and_report(&mut self, archives: &AssetArchives, outcome: &'static str) {
        if !self.active {
            return;
        }
        self.active = false;
        let wall = self.started_at.elapsed();
        let asset_reads = archives.finish_scene_loading_asset_read_performance_attribution();
        let stage_cpu = self.stages.iter().fold(Duration::ZERO, |total, stage| {
            total.saturating_add(stage.elapsed)
        });
        info!(
            target: "openzt2_scene_loading",
            scenario = ?self.scenario,
            outcome,
            wall_ms = wall.as_secs_f64() * 1_000.0,
            measured_hydration_cpu_ms = stage_cpu.as_secs_f64() * 1_000.0,
            "scene loading attribution finished"
        );
        for stage in WorldLoadingPerformanceStage::ALL {
            let attribution = self.stages[stage.index()];
            info!(
                target: "openzt2_scene_loading",
                stage = stage.name(),
                calls = attribution.calls,
                total_ms = attribution.elapsed.as_secs_f64() * 1_000.0,
                maximum_ms = attribution.maximum.as_secs_f64() * 1_000.0,
                "scene loading stage attribution"
            );
        }
        report_asset_read_performance_attribution(asset_reads);
    }
}

pub(super) struct WorldLoadingPerformanceStageTimer<'a> {
    attribution: Option<&'a mut WorldLoadingPerformanceAttribution>,
    stage: WorldLoadingPerformanceStage,
    started_at: Instant,
}

impl Drop for WorldLoadingPerformanceStageTimer<'_> {
    fn drop(&mut self) {
        let Some(attribution) = self.attribution.as_deref_mut() else {
            return;
        };
        let elapsed = self.started_at.elapsed();
        let stage = &mut attribution.stages[self.stage.index()];
        stage.calls = stage.calls.saturating_add(1);
        stage.elapsed = stage.elapsed.saturating_add(elapsed);
        stage.maximum = stage.maximum.max(elapsed);
    }
}

fn report_asset_read_performance_attribution(snapshot: SceneLoadingAssetReadPerformanceSnapshot) {
    let total_reads = snapshot
        .archive
        .reads
        .saturating_add(snapshot.loose.reads)
        .saturating_add(snapshot.memory_cache.reads);
    let total_bytes = snapshot
        .archive
        .bytes
        .saturating_add(snapshot.loose.bytes)
        .saturating_add(snapshot.memory_cache.bytes);
    let total_elapsed = snapshot
        .archive
        .elapsed
        .saturating_add(snapshot.loose.elapsed)
        .saturating_add(snapshot.memory_cache.elapsed);
    info!(
        target: "openzt2_scene_loading",
        total_reads,
        total_bytes,
        summed_read_ms = total_elapsed.as_secs_f64() * 1_000.0,
        archive_reads = snapshot.archive.reads,
        archive_bytes = snapshot.archive.bytes,
        archive_summed_read_ms = snapshot.archive.elapsed.as_secs_f64() * 1_000.0,
        archive_maximum_read_ms = snapshot.archive.maximum.as_secs_f64() * 1_000.0,
        loose_reads = snapshot.loose.reads,
        loose_bytes = snapshot.loose.bytes,
        loose_summed_read_ms = snapshot.loose.elapsed.as_secs_f64() * 1_000.0,
        loose_maximum_read_ms = snapshot.loose.maximum.as_secs_f64() * 1_000.0,
        memory_cache_reads = snapshot.memory_cache.reads,
        memory_cache_bytes = snapshot.memory_cache.bytes,
        memory_cache_summed_read_ms = snapshot.memory_cache.elapsed.as_secs_f64() * 1_000.0,
        "scene loading asset-read attribution"
    );
    let mut extensions = snapshot.extensions.into_iter().collect::<Vec<_>>();
    extensions.sort_unstable_by(|left, right| right.1.elapsed.cmp(&left.1.elapsed));
    for (extension, attribution) in extensions {
        info!(
            target: "openzt2_scene_loading",
            %extension,
            reads = attribution.reads,
            bytes = attribution.bytes,
            summed_read_ms = attribution.elapsed.as_secs_f64() * 1_000.0,
            maximum_read_ms = attribution.maximum.as_secs_f64() * 1_000.0,
            "scene loading asset-read extension attribution"
        );
    }
    let mut paths = snapshot.paths.into_iter().collect::<Vec<_>>();
    paths.sort_unstable_by(|left, right| {
        right
            .1
            .elapsed
            .cmp(&left.1.elapsed)
            .then_with(|| right.1.reads.cmp(&left.1.reads))
    });
    paths.truncate(20);
    for (path, attribution) in paths {
        info!(
            target: "openzt2_scene_loading",
            path = %path.display(),
            reads = attribution.reads,
            bytes = attribution.bytes,
            summed_read_ms = attribution.elapsed.as_secs_f64() * 1_000.0,
            maximum_read_ms = attribution.maximum.as_secs_f64() * 1_000.0,
            "scene loading cumulative asset-path attribution"
        );
    }
    let mut loaders = snapshot.loaders.into_iter().collect::<Vec<_>>();
    loaders.sort_unstable_by(|left, right| right.1.elapsed.cmp(&left.1.elapsed));
    for (loader, attribution) in loaders {
        info!(
            target: "openzt2_scene_loading",
            loader,
            loads = attribution.reads,
            summed_translation_ms = attribution.elapsed.as_secs_f64() * 1_000.0,
            maximum_translation_ms = attribution.maximum.as_secs_f64() * 1_000.0,
            "scene loading asset translation attribution"
        );
    }
    for read in snapshot.slowest {
        info!(
            target: "openzt2_scene_loading",
            path = %read.path.display(),
            source = read.source,
            bytes = read.bytes,
            elapsed_ms = read.elapsed.as_secs_f64() * 1_000.0,
            "scene loading slow asset read"
        );
    }
}
