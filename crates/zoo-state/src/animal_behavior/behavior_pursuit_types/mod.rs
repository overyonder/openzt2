//! Progress of one running `BFBehPursuit` action.

use std::time::Duration;

use bevy::prelude::*;

use crate::behavior_task_execution_types::running_actions::{
    BehaviorInstructionId, RunningBehaviorInstruction,
};
use crate::behavior_task_execution_types::BehaviorTaskExecutionState;

/// Present from the pursuit's first update until it completes, fails or its
/// task frame is left.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingBehaviorPursuit {
    pub instruction: BehaviorInstructionId,
    pub leg: BehaviorPursuitLeg,
}

#[derive(Component, Debug)]
pub struct BehaviorPursuitPoll(Timer);

impl BehaviorPursuitPoll {
    pub fn from_remaining_seconds(seconds: f32) -> Self {
        let remaining = Duration::try_from_secs_f32(seconds.max(0.0)).unwrap_or(Duration::MAX);
        // Authored polling waits until the remaining interval is strictly negative.
        Self(Timer::new(
            remaining.saturating_add(Duration::from_nanos(1)),
            TimerMode::Once,
        ))
    }

    pub fn remaining_seconds(&self) -> f32 {
        self.0
            .remaining()
            .saturating_sub(Duration::from_nanos(1))
            .as_secs_f32()
    }

    pub fn is_due(&mut self, delta: Duration) -> bool {
        !delta.is_zero() && self.0.tick(delta).is_finished()
    }
}

/// Locomotion settles direct requests in the step they are made: the agent's
/// destination then names the leg that runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BehaviorPursuitLeg {
    /// A planned route toward where the target stood when it began.
    Route { request_id: u64 },
    /// The route arrived; the next update chooses the following leg.
    RouteArrived,
    /// Direct steering was requested; a refused line runs this route instead.
    DirectRequested {
        request_id: u64,
        route_request_id: u64,
    },
    /// Steering straight at the live target. A pending line re-test names the
    /// route that replaces the steering if the line has become blocked.
    Direct {
        request_id: u64,
        refused_route_request_id: Option<u64>,
    },
    /// Direct steering reached its arrival radius; the next update completes
    /// the pursuit or resumes steering.
    DirectArrived { request_id: u64 },
}

impl BehaviorPursuitLeg {
    pub const fn navigation_request_ids(self) -> [Option<u64>; 2] {
        match self {
            Self::Route { request_id } | Self::DirectArrived { request_id } => {
                [Some(request_id), None]
            }
            Self::DirectRequested {
                request_id,
                route_request_id,
            } => [Some(request_id), Some(route_request_id)],
            Self::Direct {
                request_id,
                refused_route_request_id,
            } => [Some(request_id), refused_route_request_id],
            Self::RouteArrived => [None, None],
        }
    }

    /// Direct requests whose refusal is answered by a route, not by failure.
    pub const fn is_direct_request(self, request_id: u64) -> bool {
        match self {
            Self::DirectRequested {
                request_id: direct, ..
            }
            | Self::Direct {
                request_id: direct, ..
            } => direct == request_id,
            _ => false,
        }
    }

    /// The leg that locomotion's settled destination shows is running.
    #[must_use]
    pub const fn settled_by_destination(self, destination_request_id: u64) -> Self {
        match self {
            Self::DirectRequested {
                request_id,
                route_request_id,
            } => {
                if destination_request_id == request_id {
                    Self::Direct {
                        request_id,
                        refused_route_request_id: None,
                    }
                } else if destination_request_id == route_request_id {
                    Self::Route {
                        request_id: route_request_id,
                    }
                } else {
                    self
                }
            }
            Self::Direct {
                request_id,
                refused_route_request_id: Some(route_request_id),
            } => {
                if destination_request_id == route_request_id {
                    Self::Route {
                        request_id: route_request_id,
                    }
                } else if destination_request_id == request_id {
                    Self::Direct {
                        request_id,
                        refused_route_request_id: None,
                    }
                } else {
                    self
                }
            }
            _ => self,
        }
    }
}

impl PendingBehaviorPursuit {
    pub fn owns(
        &self,
        task: &BehaviorTaskExecutionState,
        step: &RunningBehaviorInstruction,
    ) -> bool {
        self.instruction == step.id() && step.is_current(task)
    }
}
