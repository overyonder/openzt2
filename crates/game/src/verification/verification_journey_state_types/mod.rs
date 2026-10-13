use std::collections::BTreeMap;

use bevy::{camera::RenderTarget, prelude::*};
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::{
    plugins::economy::account_transaction_types::TransactionCompleted,
    plugins::persistence::save_slot_types::{
        LoadWorldSnapshotFromSlot, WorldSnapshotLoadedFromSlot, WorldSnapshotSavedToSlot,
    },
};
use openzt2_game_assets::application_lifecycle::GamePhase;

use super::verification_journey_types::{
    VerificationJourney, VerificationJourneyStep, VerificationNavigationRoutes,
    VerificationSessionPlace,
};

/// Facts are named numbers; booleans are 0 or 1.
pub(crate) type VerificationFacts = BTreeMap<String, f64>;

/// Runs journeys back to back in one game, navigating between their starts
/// through the game's own UI.
#[derive(Resource)]
pub(crate) struct VerificationJourneySession {
    pub(crate) journeys: Box<[VerificationJourney]>,
    pub(crate) routes: VerificationNavigationRoutes,
    /// Each journey writes into `<output_root>/<journey name>/`.
    pub(crate) output_root: std::path::PathBuf,
    pub(crate) windowed: bool,
    pub(crate) capture_target: Option<RenderTarget>,
    /// The journey that is running, or being navigated to.
    pub(crate) journey_index: usize,
    pub(crate) stage: VerificationSessionStage,
    /// Steps of the current stage: a route or the journey itself.
    pub(crate) steps: Box<[VerificationJourneyStep]>,
    pub(crate) step_index: usize,
    pub(crate) step_frame: u32,
    /// Wall time the running step started, for waits on asynchronous loading.
    pub(crate) step_started_at: std::time::Instant,
    /// The whole game's wall-time budget. Once spent, the running journey fails
    /// and the game exits, leaving later journeys unrun.
    pub(crate) run_deadline: std::time::Instant,
    pub(crate) budget: std::time::Duration,
    pub(crate) budget_exhausted: bool,
    /// Game frame and time at the start of the current frame-rate window, while a
    /// journey's own steps run outside world loading.
    pub(crate) frame_rate_window: Option<(u64, std::time::Instant)>,
    /// Resolved pointer position for the running step, once its target is found.
    pub(crate) step_pointer_position: Option<Vec2>,
    /// Step frame on which the pointer target was found; input edges count from it.
    pub(crate) step_resolved_frame: Option<u32>,
    /// The node the running step aims at, and other nodes with its name to try
    /// in turn when a press would not reach it.
    pub(crate) step_target: Option<Entity>,
    pub(crate) step_alternative_targets: Vec<Entity>,
    /// Times the running step re-aimed at a node that was still moving.
    pub(crate) step_target_reaims: u32,
    pub(crate) step_dismissal: VerificationDismissalProgress,
    /// Fact value an `until steady` step last saw change, and the frame it changed on.
    pub(crate) step_steady_value: Option<(f64, u32)>,
    /// Where and on which game frame the latest scripted click pressed.
    pub(crate) last_primary_press: Option<(Vec2, u64)>,
    pub(crate) pointer_position: Vec2,
    pub(crate) step_capture: VerificationCaptureProgress,
    pub(crate) fact_collection: VerificationFactCollectionState,
    pub(crate) frame_work_started: Option<std::time::Instant>,
    pub(crate) measuring_frame_work: bool,
    pub(crate) frame_time_milliseconds: Vec<f64>,
    /// Place reached by the last navigation, with the world-load count at that time.
    /// A later world load means the session has left it.
    pub(crate) place: Option<(VerificationSessionPlace, u32)>,
    pub(crate) world_loads_started: u32,
    pub(crate) observed_phase: GamePhase,
    /// State recorded on the first clean arrival at each place.
    pub(crate) known_states: BTreeMap<VerificationSessionPlace, VerificationKnownState>,
    /// Frames this game has run, and the frame and time the current journey started.
    pub(crate) frames_run: u64,
    pub(crate) journey_started_at: Option<(u64, std::time::Instant)>,
    pub(crate) any_journey_failed: bool,
    pub(crate) finished: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum VerificationCaptureProgress {
    #[default]
    Waiting,
    Preparing {
        started_at: std::time::Instant,
    },
    Requested {
        entity: Entity,
        frame: u32,
        requested_at: std::time::Instant,
    },
    Finished {
        captured_at: std::time::Instant,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationSessionStage {
    /// Chooses how to reach the next journey's start from where the game is.
    ChoosingRoute,
    /// Returns the already loaded map to its known state without leaving it.
    ResetWithinMap,
    /// Reaches the journey's start through the main menu.
    RouteThroughMainMenu,
    Journey,
    /// Captures the failed presentation, writes the failed report, then lets the
    /// navigator choose the route to the next journey.
    FinishAfterJourneyFailure,
    /// Captures evidence of a failed route, then exits so the runner can continue
    /// the remaining journeys in a fresh game.
    ExitAfterNavigationFailure,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VerificationKnownState {
    pub(crate) visible_roles: Vec<UiDocumentRole>,
    pub(crate) facts: VerificationFacts,
}

/// Progress of a session dismissal step, which presses Escape or clicks a
/// dismiss control and waits for the UI to respond each time.
#[derive(Debug, Clone, Default)]
pub(crate) struct VerificationDismissalProgress {
    pub(crate) attempts: u32,
    pub(crate) wait: VerificationDismissalWait,
    /// The UI before the latest input, to tell whether it responded.
    pub(crate) fingerprint_before: VerificationUiFingerprint,
    /// Other visible dismiss controls to click if a press would not reach the
    /// current one, ending with the current one.
    pub(crate) dismiss_alternatives: Vec<(Entity, Vec2)>,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum VerificationDismissalWait {
    #[default]
    Assess,
    ReleaseKey {
        key_code: KeyCode,
        then: VerificationDismissalAfterRelease,
    },
    ClickControl {
        node: Entity,
        position: Vec2,
        started_frame: u32,
        /// The click was Return to Game in the in-game options.
        closes_options: bool,
    },
    /// Waits for the visible UI to change after an input.
    Response {
        since_frame: u32,
        clicked: bool,
    },
    Settle {
        until_frame: u32,
    },
    OptionsClosed {
        since_frame: u32,
        clicked_return_to_game: bool,
    },
    SimulationResumed {
        since_frame: u32,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum VerificationDismissalAfterRelease {
    Response,
    OptionsClosed,
    SimulationResumed,
}

/// What a dismissal is expected to change.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct VerificationUiFingerprint {
    pub(crate) visible_roles: Vec<UiDocumentRole>,
    pub(crate) visible_named_nodes: usize,
    pub(crate) construction_tool_active: bool,
    pub(crate) immersive_mode_active: bool,
    pub(crate) placement_previews: usize,
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
    /// Settlement messages expire between snapshots, so each journey keeps its own.
    #[serde(skip)]
    pub(crate) construction_payments: Vec<TransactionCompleted>,
    #[serde(skip)]
    pub(crate) construction_commit_failures: u32,
    /// Guest payments to the zoo for items and services during this journey.
    #[serde(skip)]
    pub(crate) guest_purchases: Vec<TransactionCompleted>,
    /// Every guest payment to the zoo during this journey, including admission,
    /// so cash checks can ignore income that arrives on the guests' own timing.
    #[serde(skip)]
    pub(crate) guest_income_cents: i64,
    /// Hunger wellness of each guest when it began using a held item.
    #[serde(skip)]
    pub(crate) guest_hunger_when_eating_began: BTreeMap<Entity, u16>,
    /// Guests whose hunger wellness rose after they began using a held item.
    #[serde(skip)]
    pub(crate) guests_fed_by_eating: std::collections::BTreeSet<Entity>,
    /// Tasks guests began during this journey.
    #[serde(skip)]
    pub(crate) guest_task_starts: u32,
    #[serde(skip)]
    pub(crate) research_payment_cents: Vec<i64>,
    /// Actual persistence messages observed throughout this journey.
    #[serde(skip)]
    pub(crate) world_snapshot_save_requests: u32,
    #[serde(skip)]
    pub(crate) world_snapshot_load_requests: u32,
    #[serde(skip)]
    pub(crate) world_snapshot_save_completions: u32,
    #[serde(skip)]
    pub(crate) world_snapshot_load_completions: u32,
    #[serde(skip)]
    pub(crate) last_world_snapshot_save_completion: Option<WorldSnapshotSavedToSlot>,
    #[serde(skip)]
    pub(crate) last_world_snapshot_load_request: Option<LoadWorldSnapshotFromSlot>,
    #[serde(skip)]
    pub(crate) last_world_snapshot_load_completion: Option<WorldSnapshotLoadedFromSlot>,
    #[serde(skip)]
    pub(crate) world_snapshot_persistence_failures: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct VerificationFailure {
    pub(crate) location: String,
    pub(crate) message: String,
}
