use bevy::prelude::{KeyCode, Vec2};

use crate::application_lifecycle::GamePhase;

/// One parsed journey file. Steps run in order; each finishes before the next starts.
#[derive(Debug, Clone)]
pub(crate) struct VerificationJourney {
    pub(crate) name: String,
    pub(crate) steps: Box<[VerificationJourneyStep]>,
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
    MovePointer(VerificationPointerTarget),
    Click(VerificationPointerTarget),
    Drag {
        from: VerificationPointerTarget,
        to: Vec2,
    },
    Key {
        key_code: KeyCode,
        repeat: u32,
    },
    /// Types characters into the focused text field without triggering key-code hotkeys.
    TypeText(String),
    Wheel {
        lines: f32,
        repeat: u32,
    },
    Snapshot(String),
    Capture(String),
    MeasureFrameTime(u32),
    ExpectFact(VerificationFactExpectation),
    /// Counts visible rows of the authored list with this name.
    ExpectListRowCount {
        list_name: String,
        comparison: VerificationFactComparison,
        expected: f64,
    },
    /// Compares a named node's shown or edited text.
    ExpectNodeText {
        node_name: String,
        expected: String,
    },
    ExpectNodeVisibility {
        node_name: String,
        visible: bool,
    },
    ExpectCampaignWorld {
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
    /// The same fact as recorded by an earlier `snapshot` or `capture` step.
    Snapshot(String),
}

pub(crate) const VERIFICATION_TARGET_WIDTH: f32 = 1600.0;
pub(crate) const VERIFICATION_TARGET_HEIGHT: f32 = 900.0;
