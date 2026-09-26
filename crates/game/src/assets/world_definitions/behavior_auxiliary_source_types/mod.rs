//! Transient typed records produced from behavior-adjacent source documents.

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentSpan;
use openzt2_game_data::{
    world_definitions::animal_shows_and_training::{TrickLevel, TrickPrerequisite},
    AssetId,
};

#[derive(Clone, Debug)]
pub(crate) struct LoweredBehaviorAuxiliary {
    /// Typed placement constraints consumed by the fossil/world-definition producer.
    pub puzzle_placement_policy: Option<PuzzlePlacementPolicy>,
    /// Typed training definitions consumed by the world catalogue and show systems.
    pub tricks: Vec<SourceTrickDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PuzzlePlacementPolicy {
    pub puzzle_root: String,
    pub entity_root: String,
    pub placeable_objects: Vec<AssetId>,
    pub non_placeable_objects: Vec<AssetId>,
    pub minimum_sonar_distance_squared: f32,
    pub maximum_sonar_distance_squared: f32,
    pub minimum_sonar_view_dot: f32,
    pub dig_distance_m: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FossilSonarPolicy {
    pub dig_distance_m: f32,
    pub minimum_distance_squared: f32,
    pub maximum_distance_squared: f32,
    pub minimum_view_dot: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceTrickDefinition {
    pub id: AssetId,
    pub localized_name: AssetId,
    pub icon: AssetId,
    pub difficulty: u16,
    pub popularity_q16: i32,
    /// Source-only ordering token. Integration compiles this to a dense display ordinal.
    pub sort_key: Option<String>,
    pub source_ordinal: u32,
    pub eligible_species: Vec<AssetId>,
    /// Parent supplying inherited levels and outcome tokens.
    pub defaults_from: Option<AssetId>,
    pub prerequisite: Option<TrickPrerequisite>,
    pub target: AssetId,
    pub levels: Vec<TrickLevel>,
    pub outcome_tokens: Option<TrickOutcomeTokens>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TrickOutcomeTokens {
    pub failure: AssetId,
    pub success: AssetId,
    pub critical: AssetId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BehaviorAuxiliaryDiagnostic {
    pub virtual_path: String,
    pub span: Option<OrderedSourceDocumentSpan>,
    pub kind: BehaviorAuxiliaryDiagnosticKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BehaviorAuxiliaryDiagnosticKind {
    UnsupportedRoot {
        root: String,
    },
    MissingField {
        owner: String,
        field: &'static str,
    },
    InvalidField {
        owner: String,
        field: &'static str,
        value: String,
    },
    UnsupportedChild {
        owner: String,
        child: String,
    },
    DuplicateDefinition {
        kind: &'static str,
        id: AssetId,
    },
}
