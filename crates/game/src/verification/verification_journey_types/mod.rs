use bevy::prelude::{GamepadButton, KeyCode, Vec2};

use openzt2_game_assets::application_lifecycle::GamePhase;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum VerificationCaptureMode {
    /// Uses the established 30-frame capture gate.
    #[default]
    Full,
    /// Captures once stable UI presentation has been submitted by the renderer.
    Ready,
}

/// One parsed journey file. Steps run in order; each finishes before the next starts.
#[derive(Debug, Clone)]
pub(crate) struct VerificationJourney {
    pub(crate) name: String,
    pub(crate) start: VerificationJourneyStart,
    pub(crate) steps: Box<[VerificationJourneyStep]>,
}

/// Where a journey's first step runs. The session reaches it through the game's own UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VerificationJourneyStart {
    pub(crate) place: VerificationSessionPlace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum VerificationSessionPlace {
    /// The splash screen of a freshly launched game.
    Boot,
    MainMenu,
    FreeformMap,
    ChallengeMap,
}

/// Ordinary UI navigation shared by journey starts and cleanup.
#[derive(Debug, Clone)]
pub(crate) struct VerificationNavigationRoutes {
    pub(crate) boot_to_main_menu: Box<[VerificationJourneyStep]>,
    pub(crate) in_game_to_main_menu: Box<[VerificationJourneyStep]>,
    pub(crate) map_selection_to_main_menu: Box<[VerificationJourneyStep]>,
    pub(crate) main_menu_to_freeform_map: Box<[VerificationJourneyStep]>,
    pub(crate) main_menu_to_challenge_map: Box<[VerificationJourneyStep]>,
}

#[derive(Debug, Clone)]
pub(crate) struct VerificationJourneyStep {
    pub(crate) source_location: String,
    pub(crate) action: VerificationJourneyAction,
}

#[derive(Debug, Clone)]
pub(crate) enum VerificationJourneyAction {
    WaitForPhase(GamePhase),
    WaitFrames(u32),
    /// Waits for a condition, recording a failure if it does not hold within the limit.
    WaitUntil {
        condition: VerificationCondition,
        frame_limit: u32,
    },
    /// Waits until a fact has not changed for `steady_frames`, and has the settled
    /// value if one is given, recording a failure if it has not by the limit.
    WaitUntilSteady {
        fact: String,
        settled_value: Option<f64>,
        steady_frames: u32,
        frame_limit: u32,
    },
    MovePointer(VerificationPointerTarget),
    Click(VerificationPointerTarget),
    Drag {
        from: VerificationPointerTarget,
        to: Vec2,
    },
    Key {
        key_code: KeyCode,
        control: bool,
        repeat: u32,
    },
    HoldKeyUntil {
        key_code: KeyCode,
        condition: VerificationCondition,
        frame_limit: u32,
    },
    ConnectGamepad,
    GamepadButton(GamepadButton),
    DisconnectGamepad,
    /// Types through physical key and text events into the focused text field.
    TypeText(String),
    Wheel {
        lines: f32,
        repeat: u32,
    },
    /// Move to the catalogue viewport and send one measured native wheel event.
    WheelToCatalogue {
        definition_key: String,
        definition: openzt2_game_data::AssetId,
        list_name: Option<String>,
    },
    Snapshot(String),
    Capture(String),
    MeasureFrameTime(u32),
    Expect(VerificationCondition),
    /// Session only: dismisses in-game activities with Escape until the in-game
    /// options open.
    ReturnInGameToOptions,
    /// Session only: dismisses in-game activities with Escape until the visible
    /// documents match the map's known state, closing the in-game options if
    /// Escape opens them, and resumes a simulation the known state had running.
    ReturnInGameToKnownState(VerificationSessionPlace),
    /// Session only: dismisses main-menu screens and dialogs until only the
    /// main menu shows.
    ReturnToMainMenuScreen,
    /// Session only: records a map's known state on first arrival and fails when
    /// a later arrival differs from it. The main menu must be at rest.
    MatchKnownState(VerificationSessionPlace),
}

#[derive(Debug, Clone)]
pub(crate) enum VerificationCondition {
    Fact(VerificationFactExpectation),
    /// Counts visible rows of the authored list with this name.
    ListRowCount {
        list_name: String,
        comparison: VerificationFactComparison,
        expected: f64,
    },
    /// Compares a named node's shown or edited text.
    NodeText {
        node_name: String,
        expected: String,
    },
    WorldZooName {
        expected: String,
    },
    NodeVisibility {
        node_name: String,
        visible: bool,
    },
    CampaignWorld {
        scenario: String,
        map: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum VerificationPointerTarget {
    /// Physical pixel position in the 1600x900 verification target.
    Position(Vec2),
    /// Nth visible, enabled UI node with this name, ordered top-to-bottom then left-to-right.
    NamedNode {
        node_name: String,
        index: usize,
    },
    ListRow {
        list_name: String,
        index: u16,
    },
    /// The visible catalogue row bound to this definition, as the purchase lists bind them.
    CatalogueEntry {
        definition_key: String,
        definition: openzt2_game_data::AssetId,
        list_name: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct VerificationFactExpectation {
    pub(crate) fact: String,
    pub(crate) comparison: VerificationFactComparison,
    pub(crate) expected: VerificationExpectedFactValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationFactComparison {
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

#[derive(Debug, Clone)]
pub(crate) enum VerificationExpectedFactValue {
    Number(f64),
    /// A fact recorded by an earlier `snapshot` or `capture` step: `@name` for
    /// the same fact, or `@name.fact` for another one.
    Snapshot {
        snapshot: String,
        fact: String,
        offset: f64,
    },
}

pub(crate) const VERIFICATION_TARGET_WIDTH: f32 = 1600.0;
pub(crate) const VERIFICATION_TARGET_HEIGHT: f32 = 900.0;
