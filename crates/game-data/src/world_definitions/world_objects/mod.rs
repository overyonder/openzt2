//! Authored world-object identity, presentation, interaction, and destruction definitions.

use crate::AssetId;

mod world_object_flag_operations;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum WorldObjectKind {
    Scenery,
    Shelter,
    Food,
    Water,
    Enrichment,
    DonationBox,
    Bin,
    Bench,
    Facility,
    Fence,
    Gate,
    Path,
    Staff,
    Guest,
    Animal,
    Tank,
    ShowStage,
    Station,
    Vehicle,
    Laboratory,
}
// Consumer-facing information-view membership resolved from the authored
// entity type hierarchy during source lowering. This is never inferred from an
// `AssetId` or display name by the game.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum WorldObjectInformationViewClass {
    Building,
    Entrance,
    Fence,
    Curb,
    ZooWall,
    Foliage,
}
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum WorldObjectBiomeAutomaticPlacementClass {
    Tree,
    Plant,
    Rock,
}
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct WorldObjectPropertyFlags(u64);

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct WorldObjectAffordanceFlags(u64);

// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct WorldObjectDestructionDefinition {
    pub effect: AssetId,
    pub audio_cue: AssetId,
    pub rubble: AssetId,
    pub ruin: AssetId,
    pub rubble_count: u16,
    pub delay_ns: u64,
    pub apply_physics: bool,
    pub force_mps: [f32; 2],
    pub pitch_degrees: f32,
    pub start_height_cm: i32,
}

// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct WorldObjectRealPhysicsWaterImpactDefinition {
    pub shape_radius_m: f32,
    pub maximum_splash_speed_mps: f32,
    pub maximum_splash_strength: f32,
}

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "authored timer bounds contain floats"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct WorldObjectPresentationAttachmentDefinition {
    /// Named physical-animation state selecting this attachment.
    pub state: AssetId,
    /// Authored attachment-node identity in the primary prefab.
    pub parent_attachment: AssetId,
    pub inherit_parent_rotation: bool,
    /// Existing native scene projected beneath that attachment node.
    pub prefab: AssetId,
    pub child_attachment: Option<AssetId>,
    /// Negative bounds disable periodic activation in the native controller.
    pub minimum_period_seconds: f32,
    pub maximum_period_seconds: f32,
    pub event_trigger: Option<AssetId>,
    /// Authored lifetime of the selected child controller, in simulation time.
    pub child_animation: Option<WorldObjectChildAnimationDefinition>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, Eq, serde::Serialize)]
pub struct WorldObjectChildAnimationDefinition {
    pub duration: std::time::Duration,
    pub auto_start: bool,
    pub looping: bool,
}
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "contained attachment states have floating timer bounds"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct WorldObjectPresentationControllerDefinition {
    pub initial_state: Option<AssetId>,
    pub default_state: Option<AssetId>,
    pub overrides_animation_setting: bool,
    pub states: Vec<WorldObjectPresentationAttachmentDefinition>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum WorldObjectContainerContent {
    Food,
    Drink,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct WorldObjectContainerQuantityDefinition {
    /// Authored initial `f_FoodLevel` in the shared Q16 authored-point unit.
    pub initial_q16: i32,
    pub content: WorldObjectContainerContent,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct WorldObjectInteractionSlotDefinition {
    pub reservation_tag: AssetId,
    /// Queue slots wait for a service slot in the same named container.
    pub is_queue: bool,
    /// Authored capacity, clamped to the native byte range; defaults to one.
    pub capacity: u8,
    /// Named mutually exclusive container slot, when authored.
    pub exclusive_identifier: AssetId,
    pub owns_contents: bool,
    pub hides_contents: bool,
    pub entrance_behavior_set: AssetId,
    pub use_behavior_set: AssetId,
    pub exit_behavior_set: AssetId,
    pub target_node_name: String,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum WorldObjectDetachDestination {
    Kill,
    Drop,
    Fall,
    Container(AssetId),
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct WorldObjectDetachActionDefinition {
    pub name: AssetId,
    pub destination: WorldObjectDetachDestination,
    pub created_objects: Vec<(AssetId, WorldObjectDetachDestination)>,
}
// This schema record may contain floating-point members and therefore cannot derive `Eq`.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "authored scale is floating point"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct WorldObjectNamedPhysicalPresentationDefinition {
    pub name: AssetId,
    pub prefab: AssetId,
    pub scale: f32,
    pub required: bool,
}

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "schema records may contain floats"
)]
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct WorldObjectDefinition {
    pub id: AssetId,
    pub kind: WorldObjectKind,
    /// The resolved source family contains a `ZTAITrickComponent`. Native
    /// isInShow is false without this capability, regardless of proximity.
    pub supports_show_tricks: bool,
    #[serde(default)]
    pub view_data: Vec<WorldObjectViewData>,
    pub detach_actions: Vec<WorldObjectDetachActionDefinition>,
    #[serde(default)]
    pub selected_ui_broadcasts: Vec<WorldObjectSelectedUiBroadcast>,
    /// Native selected-entity section, resolved from the source type hierarchy.
    #[serde(default)]
    pub information_panel: AssetId,
    pub view_class: Option<WorldObjectInformationViewClass>,
    pub biome_automatic_placement_class: Option<WorldObjectBiomeAutomaticPlacementClass>,
    pub name_key: AssetId,
    pub description_key: AssetId,
    /// Exact subject named by the authored `s_Zoopedia` hyperlink.
    pub zoopedia_subject: AssetId,
    pub prefab: AssetId,
    /// Separate authored catalogue subject selected by `ZTPlacementData` iconObj.
    /// None uses the ordinary world prefab and its presentation attachments.
    #[serde(default)]
    pub catalogue_preview_prefab: Option<AssetId>,
    /// Named child-binder states attached to primary prefab sockets. State
    /// selection is live presentation state; geometry remains in scene assets.
    pub presentation_attachments: Vec<WorldObjectPresentationControllerDefinition>,
    #[serde(default)]
    pub named_physical_presentations: Vec<WorldObjectNamedPhysicalPresentationDefinition>,
    /// Authored `BFGEntityContainerSlot` interactions, grouped by their named
    /// reservation binder and retaining the model node used for docking.
    #[serde(default)]
    pub interaction_slots: Vec<WorldObjectInteractionSlotDefinition>,
    /// Authored `BFAIEntityDataInstance f_FoodLevel` state for food/drink objects.
    #[serde(default)]
    pub container_quantity: Option<WorldObjectContainerQuantityDefinition>,
    /// Named operations borrowed directly by economy execution and price controls.
    #[serde(default)]
    pub transactions: Vec<super::economy_transactions::EconomyTransactionDefinition>,
    /// Uniform authored scale of the object's presentation component. Saved
    /// world records carry their resolved instance scale directly; this value
    /// supplies the same transform when gameplay creates a new instance.
    pub prefab_scale: f32,
    pub icon: AssetId,
    pub biomes: Vec<AssetId>,
    pub location: AssetId,
    pub preview_offset_cm: [i16; 3],
    pub preview_scale: f32,
    pub terrain_fitted: bool,
    #[serde(default)]
    pub real_physics_water_impact: Option<WorldObjectRealPhysicsWaterImpactDefinition>,
    pub price_cents: i64,
    pub upkeep_cents_per_month: i32,
    pub properties: WorldObjectPropertyFlags,
    pub affordances: WorldObjectAffordanceFlags,
    pub destruction: Option<WorldObjectDestructionDefinition>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct WorldObjectSelectedUiBroadcast {
    pub target_name: String,
    pub operation: WorldObjectSelectedUiOperation,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub enum WorldObjectSelectedUiOperation {
    SetVisible(bool),
    SetActive(bool),
}

/// Source ZTAIViewData values; absence of a named row is an action failure.
#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct WorldObjectViewData {
    pub name: AssetId,
    pub view_score: f32,
    pub tour_score: f32,
    pub donation_score: f32,
    pub entertainment_score: f32,
}
