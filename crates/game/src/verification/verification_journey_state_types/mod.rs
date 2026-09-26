use std::collections::BTreeMap;

use bevy::{camera::RenderTarget, prelude::*};

use super::verification_journey_types::VerificationJourney;

/// Facts are named numbers; booleans are 0 or 1.
pub(crate) type VerificationFacts = BTreeMap<String, f64>;

#[derive(Resource)]
pub(crate) struct VerificationJourneyRun {
    pub(crate) journey: VerificationJourney,
    pub(crate) output_directory: std::path::PathBuf,
    pub(crate) windowed: bool,
    pub(crate) capture_target: Option<RenderTarget>,
    pub(crate) step_index: usize,
    pub(crate) step_frame: u32,
    /// Resolved pointer position for the running step, once its target is found.
    pub(crate) step_pointer_position: Option<Vec2>,
    /// Step frame on which the pointer target was found; input edges count from it.
    pub(crate) step_resolved_frame: Option<u32>,
    pub(crate) pointer_position: Vec2,
    pub(crate) screenshot_pending: bool,
    pub(crate) fact_collection: VerificationFactCollectionState,
    pub(crate) frame_work_started: Option<std::time::Instant>,
    pub(crate) measuring_frame_work: bool,
    pub(crate) frame_time_milliseconds: Vec<f64>,
    pub(crate) finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationFactCollectionState {
    Idle,
    Requested,
    Collected,
}

#[derive(Resource, Default, Debug, serde::Serialize)]
pub(crate) struct VerificationJourneyReport {
    pub(crate) journey: String,
    pub(crate) passed: bool,
    pub(crate) failures: Vec<VerificationFailure>,
    pub(crate) snapshots: BTreeMap<String, VerificationFacts>,
    /// Frame-time statistics from `measure-frame-time`, also exposed as facts.
    pub(crate) frame_time_facts: VerificationFacts,
    #[serde(skip)]
    pub(crate) current_facts: VerificationFacts,
    #[serde(skip)]
    pub(crate) recorded_invariants: std::collections::BTreeSet<&'static str>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct VerificationFailure {
    pub(crate) location: String,
    pub(crate) message: String,
}
