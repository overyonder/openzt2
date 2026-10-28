//! Progress of one running `BFBehPursuit` action.

use bevy::prelude::*;

use crate::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::behavior_task_execution_types::BehaviorTaskReturnFrame;

/// Present from the pursuit's first update until it completes, fails or its
/// task frame is left.
#[derive(Component, Debug, Clone)]
pub struct PendingBehaviorPursuit {
    /// Root task execution that started the pursuit.
    pub execution_id: u64,
    pub origin: BehaviorTaskReturnFrame,
    pub stack_depth: usize,
    /// Counts down by fixed-step seconds; a negative value runs the authored
    /// poll. It starts at zero, so the first update polls.
    pub poll_remaining_seconds: f32,
    pub leg: BehaviorPursuitLeg,
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
    pub fn still_owns_exact_task_frame(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.execution_id == self.execution_id
            && task.stack.len() == self.stack_depth
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }
}
