//! Immutable world, starting-zoo, scenario, challenge, and photo contracts.

mod document_queries;
mod scenario_flag_operations;

use serde::{Deserialize, Serialize};

use crate::AssetId;

pub const WORLD_SCENARIO_VERSION: u32 = 32;

/// Maximum distinct subject identities selected across one authored photo set.
/// The shipped sequential sets top out at three independently selected types.
pub const PHOTO_DISTINCT_SUBJECT_CAPACITY: usize = 3;

/// Largest authored repetition count for one typed photo criterion.
///
/// `JeepWithGuests` requires four matching guests. This names the criterion's
/// scalar comparison bound independently from the capture buffer and from a
/// photo set's distinct-identity history.
pub const PHOTO_CRITERION_COUNT_CAPACITY: usize = 4;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct StartingZooEntityTransform {
    pub translation_m: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub scale: [f32; 3],
}

/// Scenario facts contributed by one winning source document.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct WorldScenarioDocument {
    pub locations: Vec<WorldLocationRecord>,
    pub maps: Vec<WorldMapRecord>,
    pub starting_zoos: Vec<StartingZooRecord>,
    pub campaigns: Vec<ScenarioCampaignRecord>,
    pub campaign_order: Vec<AssetId>,
    pub scenarios: Vec<ScenarioDefinitionRecord>,
    pub photo_challenges: Vec<PhotoChallengeRule>,
    pub photo_challenge_sets: Vec<PhotoChallengeSetRecord>,
    pub scenario_script_bindings: Vec<ScenarioScriptBinding>,
    pub photo_challenge_script_bindings: Vec<PhotoChallengeScriptBinding>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScenarioScriptPhase {
    Validate,
    Evaluate,
    Success,
    Failure,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ScenarioScriptBinding {
    pub scenario: AssetId,
    pub objective: AssetId,
    pub script: String,
    pub entry: String,
    pub phase: ScenarioScriptPhase,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PhotoChallengeScriptBinding {
    pub challenge: AssetId,
    pub script: String,
    pub entry: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct WorldMapSupportedGameModeFlags(u16);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct StartingZooSpawnEntityFlags(u8);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct ScenarioRecordFlags(u8);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldMapRecord {
    pub id: AssetId,
    /// Position in the resolved authored map index. Runtime lookup remains
    /// sorted by `id`; presentation uses this field to reproduce catalogue
    /// order without retaining a second map table.
    pub catalogue_order: u32,
    pub name_key: AssetId,
    pub location: AssetId,
    /// Stable authored biome identity used by map-selection filters.
    pub biome: AssetId,
    pub biome_key: AssetId,
    pub size_key: AssetId,
    /// Optional localized description shown by the map-selection presentation.
    pub description_key: Option<AssetId>,
    pub thumbnail: AssetId,
    pub environment: AssetId,
    pub camera: AssetId,
    pub terrain: AssetId,
    pub starting_zoo: AssetId,
    /// Authored `ZTMapData@xpack` selector value used by the map catalogue's
    /// expansion-family buttons. Missing values in the retail index mean the
    /// base-game family (`0`).
    pub expansion_pack_filter_identifier: u16,
    pub supported_game_modes: WorldMapSupportedGameModeFlags,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldLocationRecord {
    pub id: AssetId,
    pub name_key: AssetId,
    /// `[longitude, latitude]` in signed centidegrees.
    pub globe_marker: [i16; 2],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StartingZooRecord {
    pub id: AssetId,
    pub profile: AssetId,
    pub map: AssetId,
    /// Saved overhead-camera anchor in native `[x, y-up, z]` metres.
    pub camera_position_m: [f32; 3],
    /// Saved camera-root `[pitch, yaw, roll]` rotation in radians.
    pub camera_rotation_radians: [f32; 3],
    /// Player-visible zoo name after the original locale/default-name import
    /// rules have resolved it.
    pub name: String,
    /// Exact persisted admissions state from `ZTStatus.zoo_is_open`.
    pub admissions_open: bool,
    pub cash_cents: i64,
    /// Saved admission price. Older starting-zoo documents omit it and defer
    /// to the active guest-generation policy.
    pub admission_cents: Option<i64>,
    pub fame_half_stars: u8,
    pub maximum_fame_percent_reached: Option<f32>,
    pub start_tick: u64,
    pub absolute_day: u32,
    pub calendar: [u16; 3],
    pub imported_seed: Option<u64>,
    pub entities: Vec<StartingZooSpawnEntityRecord>,
    pub spawn_values: Vec<StartingZooSpawnValue>,
    pub paths: Vec<StartingPathRecord>,
    pub topology_nodes: Vec<StartingTopologyNodeRecord>,
    pub fences: Vec<StartingFenceRecord>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct StartingZooSpawnEntityRecord {
    pub persistent_id: u64,
    pub prefab: AssetId,
    pub definition: AssetId,
    pub transform: StartingZooEntityTransform,
    pub parent: u32,
    pub flags: StartingZooSpawnEntityFlags,
}

/// One authored path tile in a starting zoo.
///
/// Source lowering retains the exact
/// world-space centre because the selected terrain owns grid spacing; world
/// hydration reduces that position to the topology owner's integer cell.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct StartingPathRecord {
    pub persistent_id: u64,
    pub definition: AssetId,
    pub position_m: [f32; 3],
}

/// One unique authored fence endpoint in a starting zoo.
///
/// Source lowering resolves
/// legacy position/rotation entities into this shared endpoint table; world
/// hydration reduces the native world position through the active topology
/// grid and gives the endpoint ordinary ECS ownership.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct StartingTopologyNodeRecord {
    pub persistent_id: u64,
    pub position_m: [f32; 3],
}

/// One source-authored starting fence edge.
///
/// `definition` remains the authored
/// fence or gate identity until the active definition index resolves it.
/// Endpoint indexes are local to
/// the owning [`StartingZooRecord::topology_nodes`] collection.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartingFenceRecord {
    pub persistent_id: u64,
    pub definition: AssetId,
    pub a: u32,
    pub b: u32,
    pub gate: bool,
    pub protected: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum StartingZooSpawnValue {
    Name { entity: u32, text: String },
    Condition { entity: u32, permille: u16 },
    CashRegister { entity: u32, cents: i64 },
    Research { item: AssetId, progress: u16 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ScenarioCampaignRecord {
    pub id: AssetId,
    pub name_key: AssetId,
    pub description_key: AssetId,
    pub scenarios: Vec<CampaignScenarioRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CampaignScenarioRecord {
    pub id: AssetId,
    pub map: AssetId,
    pub starting_zoo: AssetId,
    pub starting_cash_cents: i64,
    pub difficulty: AssetId,
    pub name_key: AssetId,
    pub description_key: AssetId,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ScenarioDefinitionRecord {
    pub id: AssetId,
    pub name_key: AssetId,
    pub description_key: AssetId,
    pub objectives: Vec<ScenarioObjectiveRecord>,
    pub time_limit_ticks: u64,
    pub flags: ScenarioRecordFlags,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ScenarioObjectiveRecord {
    pub id: AssetId,
    /// Authored `BFScenarioRule/info@type` used by the goal-panel tabs.
    pub category: AssetId,
    /// Neutral/active, success, and failure row labels.
    pub text_key: AssetId,
    pub success_text_key: AssetId,
    pub failure_text_key: AssetId,
    pub prerequisite_objectives: Vec<AssetId>,
    pub hidden: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PhotoChallengeRule {
    pub id: AssetId,
    pub instruction_key: AssetId,
    pub completion_key: AssetId,
    pub difficulty: u8,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PhotoChallengeSetSelection {
    AllMembersInAuthoredOrder,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PhotoChallengeSetProgression {
    CompleteEachMemberOnce,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PhotoChallengeSetRecord {
    pub id: AssetId,
    pub title_key: AssetId,
    pub members: Vec<PhotoChallengeMemberRecord>,
    pub selection: PhotoChallengeSetSelection,
    pub progression: PhotoChallengeSetProgression,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhotoChallengeMemberRecord {
    pub challenge: AssetId,
    pub ordinal: u16,
}
