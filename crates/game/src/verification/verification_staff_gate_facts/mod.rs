use bevy::{
    ecs::system::SystemParam,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_data::{world_definitions::staff_management::StaffRoleKind, AssetId};

use crate::plugins::{
    animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner,
    animation_playback::animation_playback_controller_types::AnimationPlaybackController,
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::{HabitatIndex, HabitatRegion},
    },
    locomotion::locomotion_types::{Destination, NavAgent, NavFlags, Route},
    staff::{
        staff_employment_types::StaffRole,
        staff_job_types::{CurrentJob, JobClaim, StaffJobBehaviorTask},
    },
    topology::{
        gate_actor_type_queries::GateActorTypeQueries,
        gate_operation_types::Gate,
        topology_graph_types::{FenceEdge, TopologyGrid, TopologyNode},
        topology_grid_geometry::horizontal_world_segments_intersect,
    },
};

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
    VerificationSessionStage,
};

/// Observed routes and motion, never gameplay requests or replacement state.
#[derive(Resource, Default)]
pub(super) struct VerificationStaffGateEvidence {
    journey: Option<(usize, u64)>,
    closed_routes: HashSet<(Entity, u64, Entity)>,
    crossings: HashSet<(Entity, Entity)>,
    opened: HashSet<Entity>,
    reclosed: HashSet<Entity>,
    previous_positions: HashMap<Entity, Vec3>,
}

#[derive(SystemParam)]
pub(super) struct VerificationStaffGateQueries<'w, 's> {
    types: GateActorTypeQueries<'w, 's>,
    gates: Query<
        'w,
        's,
        (
            Entity,
            &'static Gate,
            &'static FenceEdge,
            &'static GlobalTransform,
        ),
    >,
    agents: Query<'w, 's, (Entity, &'static GlobalTransform, &'static NavAgent)>,
    nodes: Query<'w, 's, &'static TopologyNode>,
    grid: Res<'w, TopologyGrid>,
    index: Res<'w, HabitatIndex>,
    habitats: Query<'w, 's, (Entity, &'static HabitatRegion)>,
    workers: Query<
        'w,
        's,
        (
            Entity,
            &'static StaffRole,
            &'static GlobalTransform,
            Option<&'static Route>,
            Option<&'static Destination>,
            Option<&'static CurrentJob>,
        ),
    >,
    claims: Query<'w, 's, (&'static JobClaim, &'static StaffJobBehaviorTask)>,
    animations: Query<
        'w,
        's,
        (
            &'static AnimationPresentationOwner,
            &'static AnimationPlaybackController,
        ),
    >,
}

fn prepare_gate_evidence(
    session: &VerificationJourneySession,
    evidence: &mut VerificationStaffGateEvidence,
) -> bool {
    if session.stage != VerificationSessionStage::Journey
        || !session
            .journeys
            .get(session.journey_index)
            .is_some_and(|journey| journey.name == "populated-zoo")
    {
        return false;
    }
    let identity = session
        .journey_started_at
        .map(|(frame, _)| (session.journey_index, frame));
    if evidence.journey != identity {
        *evidence = VerificationStaffGateEvidence {
            journey: identity,
            ..Default::default()
        };
    }
    true
}

impl VerificationStaffGateQueries<'_, '_> {
    fn is_keeper(&self, role: &StaffRole) -> bool {
        self.types
            .definitions()
            .and_then(|definitions| definitions.find_staff(role.0))
            .is_some_and(|role| role.role == StaffRoleKind::Keeper)
    }

    fn gate_belongs_to_habitat(&self, gate: Entity) -> bool {
        self.gates
            .get(gate)
            .ok()
            .and_then(|(_, _, edge, _)| self.types.definitions()?.find_fence(edge.definition))
            .is_some_and(|definition| {
                definition.gate_policy.open_for_entity_type == Some(AssetId::from_key("staff"))
            })
            && self
                .habitats
                .iter()
                .any(|(_, region)| region.boundary_fence_entities.contains(&gate))
    }

    fn segment_crosses_gate(&self, from: Vec3, to: Vec3, edge: &FenceEdge) -> bool {
        let (Ok(a), Ok(b)) = (self.nodes.get(edge.a), self.nodes.get(edge.b)) else {
            return false;
        };
        horizontal_world_segments_intersect(
            from.xz(),
            to.xz(),
            self.grid.cell_translation(a.cell).xz(),
            self.grid.cell_translation(b.cell).xz(),
        )
    }

    fn is_feed_claim(&self, worker: Entity, job: Option<&CurrentJob>) -> bool {
        job.and_then(|job| self.claims.get(job.job).ok())
            .is_some_and(|(claim, task)| {
                claim.staff == worker
                    && task.task == AssetId::from_key("keeper:feedanimal")
                    && task.execution_id.is_some()
            })
    }
}

/// Observe planned routes before the Act set can open their gate in the same tick.
pub(super) fn observe_keeper_routes_before_gate_operation(
    session: Res<VerificationJourneySession>,
    mut evidence: ResMut<VerificationStaffGateEvidence>,
    world: VerificationStaffGateQueries,
) {
    if !prepare_gate_evidence(&session, &mut evidence) {
        return;
    }
    for (gate_entity, gate, edge, _) in &world.gates {
        if !world.gate_belongs_to_habitat(gate_entity) {
            continue;
        }
        if gate.open {
            evidence.opened.insert(gate_entity);
        } else if evidence.opened.contains(&gate_entity) {
            evidence.reclosed.insert(gate_entity);
        }
        if gate.open {
            continue;
        }
        for (worker, role, transform, route, destination, job) in &world.workers {
            if !world.is_keeper(role) || !world.is_feed_claim(worker, job) {
                continue;
            }
            let (Some(route), Some(destination)) = (route, destination) else {
                continue;
            };
            let mut previous = transform.translation();
            if route
                .points
                .get(route.cursor..)
                .unwrap_or_default()
                .iter()
                .copied()
                .any(|point| {
                    let crosses = world.segment_crosses_gate(previous, point, edge);
                    previous = point;
                    crosses
                })
            {
                evidence
                    .closed_routes
                    .insert((worker, destination.request_id, gate_entity));
            }
        }
    }
}

pub(super) fn collect_staff_gate_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    mut evidence: ResMut<VerificationStaffGateEvidence>,
    world: VerificationStaffGateQueries,
) {
    if !prepare_gate_evidence(&session, &mut evidence) {
        return;
    }
    let mut keepers_inside = 0;
    let mut feed_claims = 0;
    for (worker, role, transform, _, _, job) in &world.workers {
        if !world.is_keeper(role) {
            continue;
        }
        feed_claims += usize::from(world.is_feed_claim(worker, job));
        let position = transform.translation();
        let habitat = locate_habitat_at_world_position(&world.index, position.xz(), *world.grid);
        let in_gated_habitat = habitat
            .and_then(|habitat| world.habitats.get(habitat).ok())
            .is_some_and(|(_, region)| {
                region
                    .boundary_fence_entities
                    .iter()
                    .any(|gate| world.gate_belongs_to_habitat(*gate))
            });
        keepers_inside += usize::from(in_gated_habitat);
        if let Some(previous) = evidence.previous_positions.insert(worker, position) {
            if in_gated_habitat
                && locate_habitat_at_world_position(&world.index, previous.xz(), *world.grid)
                    != habitat
            {
                for (gate_entity, gate, edge, _) in &world.gates {
                    if gate.open
                        && world.gate_belongs_to_habitat(gate_entity)
                        && world.segment_crosses_gate(previous, position, edge)
                    {
                        evidence.crossings.insert((worker, gate_entity));
                    }
                }
            }
        }
    }
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let mut gate_count = 0;
    let mut open_count = 0;
    let mut open_clips = 0;
    let mut closed_clips = 0;
    let mut ineligible_only_closed_gates = 0;
    for (gate_entity, gate, edge, transform) in &world.gates {
        if !world.gate_belongs_to_habitat(gate_entity) {
            continue;
        }
        gate_count += 1;
        open_count += usize::from(gate.open);
        let Some(definition) = world
            .types
            .definitions()
            .and_then(|definitions| definitions.find_fence(edge.definition))
        else {
            continue;
        };
        let policy = &definition.gate_policy;
        if !gate.open && policy.open_for_entity_type.is_some() {
            let position = transform.translation().xz();
            let open_distance = f32::from(policy.trigger_distance_cm) * 0.01;
            let close_distance = f32::from(policy.close_distance_cm) * 0.01;
            let required_type = policy
                .open_for_entity_type
                .expect("checked authored opener type");
            let animal_near = world.agents.iter().any(|(actor, transform, agent)| {
                agent.capabilities.contains(NavFlags::ANIMAL)
                    && !world.types.actor_types(actor).contains(&required_type)
                    && transform.translation().xz().distance_squared(position)
                        <= open_distance * open_distance
            });
            let eligible_near = world.agents.iter().any(|(actor, transform, _)| {
                world.types.actor_types(actor).contains(&required_type)
                    && transform.translation().xz().distance_squared(position)
                        <= close_distance * close_distance
            });
            ineligible_only_closed_gates += usize::from(animal_near && !eligible_near);
        }
        let clip = if gate.open {
            &definition.gate_policy.open_animation
        } else {
            &definition.gate_policy.close_animation
        };
        let matching_clip = world.animations.iter().any(|(owner, controller)| {
            owner.gameplay_entity == gate_entity
                && controller.explicit_clip_request_id.is_some()
                && controller.animation_clip_asset_key == *clip
        });
        if gate.open {
            open_clips += usize::from(matching_clip);
        } else {
            closed_clips += usize::from(matching_clip);
        }
    }
    for (name, count) in [
        ("habitat_staff_gates", gate_count),
        ("open_habitat_staff_gates", open_count),
        ("keeper_feed_claims", feed_claims),
        ("keeper_closed_gate_routes", evidence.closed_routes.len()),
        ("keeper_gate_crossings", evidence.crossings.len()),
        ("keepers_inside_staff_gate_habitats", keepers_inside),
        ("staff_gate_reclosures", evidence.reclosed.len()),
        ("open_staff_gate_clips", open_clips),
        ("closed_staff_gate_clips", closed_clips),
        (
            "ineligible_only_closed_staff_gates",
            ineligible_only_closed_gates,
        ),
    ] {
        report.current_facts.insert(
            name.to_owned(),
            f64::from(u32::try_from(count).unwrap_or(u32::MAX)),
        );
    }
}
