//! Observations of the fence terrain fitter, installed only by runtime verification.

use bevy::asset::AssetId;
use bevy::ecs::query::QueryEntityError;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use crate::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::terrain::terrain_chunk_types::TerrainChunk;
use crate::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use crate::topology::topology_graph_types::FenceEdge;
use crate::topology::topology_graph_types::TopologyGrid;

const FENCE_FITTING_TRANSITION_LIMIT: u8 = 16;

/// Bounded fitting transitions for the four observed fence owners within one
/// journey world. Verification sets the journey scope; outside a running
/// journey no history is kept.
#[derive(Resource, Default)]
pub struct FenceTerrainFittingHistory {
    journey_scope: Option<(usize, u32)>,
    lifetime: Option<(usize, u32, Entity)>,
    owners: [FenceTerrainFittingOwnerHistory; 4],
}

impl FenceTerrainFittingHistory {
    /// Sets the running journey index and its world load count, or `None` once
    /// the journey session has finished.
    pub const fn set_journey_scope(&mut self, journey_scope: Option<(usize, u32)>) {
        self.journey_scope = journey_scope;
    }

    pub const fn lifetime(&self) -> Option<(usize, u32, Entity)> {
        self.lifetime
    }

    pub fn emitted_transitions(&self) -> [u8; 4] {
        self.owners.each_ref().map(|owner| owner.emitted)
    }

    pub fn omitted_transitions(&self) -> [u64; 4] {
        self.owners.each_ref().map(|owner| owner.omitted)
    }
}

#[derive(Default)]
struct FenceTerrainFittingOwnerHistory {
    last: Option<FenceTerrainFittingAttempt>,
    attempt_seen: bool,
    emitted: u8,
    omitted: u64,
}

#[derive(Debug, PartialEq)]
pub struct FenceTerrainFittingAttempt {
    slot: usize,
    persistent_id: u64,
    root: Entity,
    entity: Entity,
    endpoints: [IVec3; 2],
    skew_entities: [Entity; 2],
    triggers: [bool; 4],
    horizontal_length_m: f32,
    owner_before: Transform,
    owner_after: Transform,
    skew_before: [Option<Transform>; 2],
    skew_after: [Option<Transform>; 2],
    samples: [Option<FenceTerrainEndpointSample>; 2],
    child_query_result: Option<Result<(), QueryEntityError>>,
    outcome: &'static str,
}

impl FenceTerrainFittingAttempt {
    pub const fn record_child_query_result(&mut self, result: Result<(), QueryEntityError>) {
        self.child_query_result = Some(result);
    }
}

#[derive(Debug, PartialEq)]
struct FenceTerrainEndpointSample {
    world_xz: Vec2,
    index_origin: Vec2,
    index_span_m: f32,
    chunk_entity: Option<Entity>,
    chunk_root: Option<Entity>,
    chunk_coord: Option<IVec2>,
    source_coord: Option<IVec2>,
    terrain_asset: Option<AssetId<TerrainAsset>>,
    edited_overlay_available: bool,
    height_m: Option<f32>,
    availability: &'static str,
}

#[derive(SystemParam)]
pub struct FenceTerrainFittingObservation<'w, 's> {
    history: Option<ResMut<'w, FenceTerrainFittingHistory>>,
    roots: Query<'w, 's, Entity, (With<WorldRoot>, With<SelectedWorldIdentity>)>,
    owners: Query<'w, 's, (&'static PersistentId, &'static WorldMember), With<FenceEdge>>,
    chunk_members: Query<'w, 's, &'static WorldMember, With<TerrainChunk>>,
    server: Option<Res<'w, AssetServer>>,
}

impl FenceTerrainFittingObservation<'_, '_> {
    pub fn begin_frame(&mut self) {
        let Some(history) = self.history.as_mut() else {
            return;
        };
        let lifetime = history
            .journey_scope
            .and_then(|(journey_index, world_loads_started)| {
                self.roots
                    .single()
                    .ok()
                    .map(|root| (journey_index, world_loads_started, root))
            });
        if history.lifetime != lifetime {
            info!(target: "openzt2_verification", previous = ?history.lifetime, current = ?lifetime,
                omitted = ?history.owners.each_ref().map(|owner| owner.omitted),
                "fence fitting observation lifetime changed; history discarded");
            **history = FenceTerrainFittingHistory {
                journey_scope: history.journey_scope,
                lifetime,
                ..default()
            };
        }
    }

    pub fn observes_owner(&self, entity: Entity) -> bool {
        let Some((_, _, root)) = self.history.as_ref().and_then(|history| history.lifetime) else {
            return false;
        };
        self.owners
            .get(entity)
            .is_ok_and(|(id, member)| member.root == root && matches!(id.0, 3 | 5 | 7 | 9))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_owner(
        &self,
        entity: Entity,
        endpoints: [IVec3; 2],
        skew_entities: [Entity; 2],
        triggers: [bool; 4],
        horizontal_length_m: f32,
        owner: Transform,
        skew: [Option<Transform>; 2],
    ) -> Option<FenceTerrainFittingAttempt> {
        if !self.observes_owner(entity) {
            return None;
        }
        let (id, member) = self.owners.get(entity).ok()?;
        let slot = match id.0 {
            3 => 0,
            5 => 1,
            7 => 2,
            9 => 3,
            _ => return None,
        };
        Some(FenceTerrainFittingAttempt {
            slot,
            persistent_id: id.0,
            root: member.root,
            entity,
            endpoints,
            skew_entities,
            triggers,
            horizontal_length_m,
            owner_before: owner,
            owner_after: owner,
            skew_before: skew,
            skew_after: skew,
            samples: [None, None],
            child_query_result: None,
            outcome: "not finished",
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_endpoint_sample(
        &self,
        attempt: &mut FenceTerrainFittingAttempt,
        endpoint: usize,
        world_xz: Vec2,
        index: &TerrainIndex,
        chunk_entity: Option<Entity>,
        chunk: Option<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
        asset_available: bool,
        height_m: Option<f32>,
    ) {
        attempt.samples[endpoint] = Some(FenceTerrainEndpointSample {
            world_xz,
            index_origin: index.origin,
            index_span_m: index.chunk_span_m,
            chunk_entity,
            chunk_root: chunk_entity
                .and_then(|entity| self.chunk_members.get(entity).ok())
                .map(|member| member.root),
            source_coord: chunk.map(|(chunk, _)| chunk.source_coord),
            chunk_coord: chunk.map(|(chunk, _)| chunk.coord),
            terrain_asset: chunk.map(|(chunk, _)| chunk.asset.id()),
            edited_overlay_available: chunk.is_some_and(|(_, edited)| edited.is_some()),
            height_m,
            availability: if chunk_entity.is_none() {
                "spatial lookup unavailable"
            } else if chunk.is_none() {
                "indexed chunk unavailable"
            } else if !asset_available {
                "terrain asset unavailable"
            } else if height_m.is_none() {
                "canonical sampler returned none"
            } else {
                "sample available"
            },
        });
    }

    pub fn finish_owner(
        &mut self,
        attempt: Option<FenceTerrainFittingAttempt>,
        outcome: &'static str,
        owner: Transform,
        skew: [Option<Transform>; 2],
        frame: Option<u32>,
    ) {
        let (Some(mut attempt), Some(history)) = (attempt, self.history.as_mut()) else {
            return;
        };
        attempt.outcome = outcome;
        attempt.owner_after = owner;
        attempt.skew_after = skew;
        let lifetime = history.lifetime;
        let previous = &mut history.owners[attempt.slot];
        let first_membership = previous.last.as_ref().is_none_or(|last| {
            last.entity != attempt.entity
                || last.root != attempt.root
                || last.endpoints != attempt.endpoints
        });
        if first_membership {
            previous.attempt_seen = false;
        }
        let production_attempted = attempt.triggers.iter().any(|trigger| *trigger);
        let first_attempt = production_attempted && !previous.attempt_seen;
        previous.attempt_seen |= production_attempted;
        let sampleable = |attempt: &FenceTerrainFittingAttempt| {
            attempt.samples.iter().all(|sample| {
                sample
                    .as_ref()
                    .is_some_and(|sample| sample.height_m.is_some())
            })
        };
        let newly_observed_sampleable =
            sampleable(&attempt) && previous.last.as_ref().is_none_or(|last| !sampleable(last));
        if previous.last.as_ref() == Some(&attempt)
            || previous.emitted >= FENCE_FITTING_TRANSITION_LIMIT
        {
            previous.omitted = previous.omitted.saturating_add(1);
        } else {
            let production_fit_completed = outcome == "production fit completed";
            info!(target: "openzt2_verification", ?frame,
                journey = ?lifetime.map(|(journey, ..)| journey),
                first_membership, first_attempt, newly_observed_sampleable,
                terrain_changed = attempt.triggers[0], endpoint_cells_changed = attempt.triggers[1],
                skew_presentation_changed = attempt.triggers[2], topology_grid_changed = attempt.triggers[3],
                production_gate_skipped = !attempt.triggers.iter().any(|trigger| *trigger),
                production_fit_completed, observation = ?attempt,
                terrain_paths = ?attempt.samples.each_ref().map(|sample| sample.as_ref().and_then(|sample| sample.terrain_asset).and_then(|asset| self.server.as_ref().and_then(|server| server.get_path(asset)))),
                same_world_chunks = ?attempt.samples.each_ref().map(|sample| sample.as_ref().map(|sample| sample.chunk_root.map(|root| root == attempt.root))),
                omitted_observations = previous.omitted,
                transition_limit = FENCE_FITTING_TRANSITION_LIMIT,
                omissions = "other fence IDs, owners outside current world and missing fitter query membership excluded; None sample means endpoint not reached; skipped-gate samples are observation only; sampler rejection substage unavailable",
                "sampled imported fence fitting transition; no entrance identity");
            previous.emitted += 1;
            if previous.emitted == FENCE_FITTING_TRANSITION_LIMIT {
                info!(target: "openzt2_verification", root = ?attempt.root, persistent_id = attempt.persistent_id,
                    "fence fitting transition limit reached; subsequent records omitted until lifetime reset");
            }
        }
        previous.last = Some(attempt);
    }
}

/// Endpoint fitting results reported in fitting order, installed only by
/// runtime verification and drained after each fitting pass.
#[derive(Resource, Default)]
pub struct FenceEndpointFittingReports {
    reports: Vec<FenceEndpointFittingReport>,
}

impl FenceEndpointFittingReports {
    pub fn record(&mut self, report: FenceEndpointFittingReport) {
        self.reports.push(report);
    }

    pub fn drain(&mut self) -> std::vec::Drain<'_, FenceEndpointFittingReport> {
        self.reports.drain(..)
    }
}

/// One fitting attempt for a fence owner and, when it completed, the two
/// fitted endpoint surfaces.
pub struct FenceEndpointFittingReport {
    pub edge: Entity,
    pub endpoint_cells: [IVec3; 2],
    pub grid: TopologyGrid,
    pub frame: Option<u32>,
    pub surfaces: Option<(Vec3, Vec3)>,
    pub status: &'static str,
}
