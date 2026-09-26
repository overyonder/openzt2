use super::{
    action::information::InformationGraphType,
    globe_presentation::UiGlobePresentationDefinition,
    node_layout::UiNodeRegionDefinition,
    overview_map_presentation::{UiMapColorsRecord, UiMapLayerRecord},
    timed_presentation::{UiCreditsCardRecord, UiLocalizedCountdownRecord, UiTimedEventRecord},
    widget_control::{
        UiAxis, UiButtonRecord, UiDragOperation, UiGridRecord, UiSliderRecord, UiTextRecord,
        UiTooltipPresentation,
    },
    widget_live_collection::UiWidgetLiveCollectionSource,
};
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub enum UiWidgetRecord {
    Plain,
    /// One source-authored surface chooser embedded by a canonical biome
    /// definition and composed into the in-game terrain panel during asset loading.
    BiomePanel {
        biome: AssetId,
    },
    Grid(UiGridRecord),
    Button(UiButtonRecord),
    List {
        grid: UiGridRecord,
        source: UiWidgetLiveCollectionSource,
        /// Typed UI fragment document used for each live row.
        row_document: AssetId,
        opener_node: AssetId,
        drop_list_display_node: AssetId,
        count_component: AssetId,
        update_seconds: f32,
    },
    Slider(UiSliderRecord),
    Drag {
        minimum_width: i32,
        maximum_width: i32,
        minimum_height: i32,
        maximum_height: i32,
        bounded: bool,
    },
    DragCommand {
        operation: UiDragOperation,
        axis: UiAxis,
        flip_axis: bool,
    },
    Graph {
        graph_type: Option<InformationGraphType>,
        forced_minimum_y: f32,
        forced_maximum_y: f32,
        force_y_values: bool,
        x_labels: i32,
        y_labels: i32,
    },
    TypeList {
        grid: UiGridRecord,
        included_kinds: Vec<AssetId>,
        excluded_kinds: Vec<AssetId>,
        row_document: AssetId,
        filters: Vec<AssetId>,
    },
    /// Source-authored `ZTAdoptionPanel`: a dynamic presentation over the
    /// canonical species catalogue, using the authored adoption-row fragment.
    AdoptionList {
        grid: UiGridRecord,
        row_document: AssetId,
    },
    FinanceList {
        grid: UiGridRecord,
        row_document: AssetId,
        labels: bool,
    },
    /// `ZTBuyInfoPanel` details for the selected catalogue entry.
    CatalogueDetails {
        grid: UiGridRecord,
    },
    WorldMap {
        layers: Vec<UiMapLayerRecord>,
        colors: UiMapColorsRecord,
    },
    MultiIcon {
        entries: Vec<UiImageEntryRecord>,
    },
    LocalizedCountdown(UiLocalizedCountdownRecord),
    TimedSequence {
        events: Vec<UiTimedEventRecord>,
    },
    CreditsSequence {
        cards: Vec<UiCreditsCardRecord>,
        repeat_after_ms: u32,
        suspend_rail_camera: bool,
    },
    EarthquakeTrigger,
    EarthquakeRunner,
    FirstPersonCountdown {
        three: AssetId,
        two: AssetId,
        one: AssetId,
        go: AssetId,
    },
    ButtonPulser {
        target: AssetId,
        hidden_ms: u32,
        visible_ms: u32,
    },
    Tool {
        tool: AssetId,
    },
    XmlEdit {
        document: AssetId,
    },
    Text(UiTextRecord),
    TextEdit {
        text: UiTextRecord,
        maximum_length: i32,
        legal_filename_only: bool,
        highlight: bool,
        change_sound: AssetId,
        error_sound: AssetId,
        cursor_on_seconds: f32,
        cursor_off_seconds: f32,
        changed_message: AssetId,
    },
    ToggleSet {
        grid: UiGridRecord,
        allow_repress: bool,
        initial_column: i32,
        source: UiWidgetLiveCollectionSource,
    },
    Globe(UiGlobePresentationDefinition),
    RailCamera {
        scene: AssetId,
    },
    Tooltip {
        text: UiTextRecord,
        presentation: UiTooltipPresentation,
        floating: bool,
        autohide: bool,
        offset: [i32; 2],
        appear_seconds: f32,
        display_seconds: f32,
    },
    TreeElement {
        grid: UiGridRecord,
        item: AssetId,
        expanded: bool,
        selected: bool,
    },
    Window {
        modal: bool,
        draggable: bool,
        horizontal: bool,
        vertical: bool,
        wheel_scroll: bool,
        title: AssetId,
        horizontal_scroll: AssetId,
        vertical_scroll: AssetId,
    },
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct UiImageEntryRecord {
    /// Source-authored semantic selector, such as `Critical` or `LowRisk`.
    pub key: AssetId,
    /// Source-authored localized description associated with this image.
    pub localization_key: AssetId,
    pub image: AssetId,
    pub source_rect: UiNodeRegionDefinition,
}
