//! Parsed UI XML/BFXML records.

use crate::source_document::{
    ordered_source_document_types::OrderedSourceDocumentSpan, path::AssetPath,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiNode {
    pub kind: SourceUiWidgetKind,
    pub name: Option<String>,
    pub template: Option<String>,
    pub template_name: Option<String>,
    /// Lowered semantic for a scenario objective row variant.
    ///
    /// This field is introduced by UI source normalization when it lowers
    /// `ZTGoalPanel`; it never represents arbitrary original UI vocabulary.
    pub scenario_objective_visual: Option<String>,
    pub cursor: Option<String>,
    pub modal: Option<bool>,
    pub always_hit: Option<SourceUiHitPolicy>,
    pub cacheable_from_lua: Option<bool>,
    pub use_list_box_color_if_in_container: Option<bool>,
    /// Semantic clipping supplied by list controls, including through a
    /// template chain. This is derived from widget kind rather than guessed
    /// from a particular document or node name.
    pub clip_children: bool,
    /// Original expansion-availability guard. The source loader asks the
    /// application whether this numbered pack is available before constructing
    /// the widget.
    pub x_pack: Option<u32>,
    pub background_layout: Option<SourceUiBackgroundLayout>,
    pub region: Option<SourceUiRegion>,
    pub state: SourceUiState,
    pub aspect: Option<SourceUiAspect>,
    pub show_hide_animation: Option<SourceUiShowHideAnimation>,
    pub notify_animation_completed: Option<bool>,
    pub help: Option<SourceUiHelp>,
    pub text_format: Option<String>,
    pub widget: SourceUiWidgetData,
    pub fields: Vec<SourceUiField>,
    pub finance_categories: Vec<SourceUiFinanceCategory>,
    pub hotkeys: Vec<SourceUiHotkey>,
    /// `<UIHotKeys><file name node/>`: a named mode in a hotkey document whose
    /// bindings belong to this node.
    pub hotkey_mode_references: Vec<SourceUiHotkeyModeReference>,
    pub events: Vec<SourceUiEventBlock>,
    pub children: Vec<SourceUiNode>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
    pub unknown_elements: Vec<SourceUiPayloadNode>,
    pub span: OrderedSourceDocumentSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiBackgroundLayout {
    pub image: AssetPath,
    pub source: [i32; 4],
    pub padding: [i32; 2],
    pub width: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceUiWidgetKind {
    Animation,
    App,
    Button,
    CompositeButton,
    CompositeHoverButton,
    Container,
    ContextList,
    Drag,
    DragCommand,
    DropList,
    FullscreenButton,
    Globe,
    GoalPanel,
    Graph,
    HoverButton,
    Layout,
    ListBox,
    MultiIcon,
    MultiList,
    PhotoAlbum,
    PushButton,
    RailCamera,
    Root,
    Slider,
    SoundManager,
    Static,
    Text,
    TextBuffer,
    TextEdit,
    TimedEvents,
    ToggleButton,
    ToggleHoverButton,
    ToggleSet,
    Tool,
    Tooltip,
    TreeElement,
    TypeList,
    Window,
    WorldMap,
    XmlEdit,
    Unknown(String),
}

impl SourceUiWidgetKind {
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "UIAnimation" => Self::Animation,
            "UIApp" => Self::App,
            "UIButton" => Self::Button,
            "UICompositeButton" => Self::CompositeButton,
            "UICompositeHoverButton" => Self::CompositeHoverButton,
            "UIContainer" => Self::Container,
            "ZTUIContextList" => Self::ContextList,
            "UIDrag" => Self::Drag,
            "UIDragCommand" => Self::DragCommand,
            "UIDropList" => Self::DropList,
            "ZTUIFullscreenButton" => Self::FullscreenButton,
            "ZTGlobe" => Self::Globe,
            "ZTGoalPanel" => Self::GoalPanel,
            "UIGraph" => Self::Graph,
            "UIHoverButton" => Self::HoverButton,
            "UILayout" | "ZTEOMComponent" => Self::Layout,
            "UIListBox" => Self::ListBox,
            "UIMultiIcon" => Self::MultiIcon,
            "ZTMultiList" => Self::MultiList,
            "ZTPhotoAlbumComponent" => Self::PhotoAlbum,
            "UIPushButton" => Self::PushButton,
            "ZTRailCam" => Self::RailCamera,
            "UIRoot" => Self::Root,
            "UISlider" => Self::Slider,
            "UISoundMgr" => Self::SoundManager,
            "UIStatic" => Self::Static,
            "UIText" => Self::Text,
            "UITextBuffer" => Self::TextBuffer,
            "UITextEdit" => Self::TextEdit,
            "ZTUITimedEvents" => Self::TimedEvents,
            "UIToggleButton" => Self::ToggleButton,
            "UIToggleHoverButton" => Self::ToggleHoverButton,
            "UIToggleSet" => Self::ToggleSet,
            "UITool" => Self::Tool,
            "UITooltip" => Self::Tooltip,
            "UITreeElement" => Self::TreeElement,
            "ZTUITypeList" => Self::TypeList,
            "UIWindow" => Self::Window,
            "ZTOverviewComponent" => Self::WorldMap,
            "UIXMLEdit" => Self::XmlEdit,
            other => Self::Unknown(other.to_owned()),
        }
    }

    pub fn is_known_tag(tag: &str) -> bool {
        !matches!(Self::from_tag(tag), Self::Unknown(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourceUiWidgetData {
    Plain,
    Layout(SourceUiGrid),
    RailCamera {
        scene: AssetPath,
    },
    Button(SourceUiButton),
    CompositeButton {
        button: SourceUiButton,
        child_button: Option<String>,
        hover_child: Option<String>,
    },
    List(SourceUiList),
    DropList {
        list: SourceUiList,
        layout: Option<String>,
        horizontal_layout: Option<String>,
        opener: Option<String>,
        drop_list: Option<String>,
    },
    Slider(SourceUiSlider),
    Drag(SourceUiDrag),
    DragCommand(SourceUiDragCommand),
    Graph(SourceUiGraph),
    MultiIcon(Vec<SourceUiImageEntry>),
    WorldMap(SourceUiWorldMap),
    Text(SourceUiText),
    TextEdit {
        text: SourceUiText,
        edit: SourceUiTextEdit,
    },
    ToggleSet {
        grid: SourceUiGrid,
        allow_repress: bool,
        initial_column: i32,
    },
    Globe(SourceUiGlobe),
    Tool {
        command: Option<String>,
    },
    Tooltip(SourceUiTooltip),
    TreeElement(SourceUiTreeElement),
    TimedEvents(Vec<SourceUiTimedEvent>),
    TypeList(SourceUiTypeList),
    Window(SourceUiWindow),
    XmlEdit {
        document: Option<AssetPath>,
    },
    Animation(SourceUiAnimation),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiRegion {
    pub x: SourceUiMetric,
    pub y: SourceUiMetric,
    pub width: SourceUiMetric,
    pub height: SourceUiMetric,
    pub x_align: SourceUiAlignment,
    pub y_align: SourceUiAlignment,
    pub width_align: SourceUiAlignment,
    pub height_align: SourceUiAlignment,
    pub unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourceUiMetric {
    Number(f32),
    Token(String),
}

impl SourceUiMetric {
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            None => Self::Number(0.0),
            Some(value) if matches!(value.trim(), "" | "-") => Self::Number(0.0),
            Some(value) => value
                .trim()
                .parse::<f32>()
                .map(Self::Number)
                .unwrap_or_else(|_| Self::Token(value.trim().to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceUiAlignment {
    Min,
    Max,
    Length,
    Mid,
    PercentMin,
    PercentMax,
    Unknown(String),
}

impl SourceUiAlignment {
    pub fn parse(value: Option<&str>, default: Self) -> Self {
        let Some(value) = value else { return default };
        let normalized = value.split_whitespace().collect::<String>();
        if normalized.is_empty() {
            return default;
        }
        match normalized.as_str() {
            "min" => Self::Min,
            "max" => Self::Max,
            "len" => Self::Length,
            "mid" => Self::Mid,
            "%min" => Self::PercentMin,
            "%max" => Self::PercentMax,
            _ => Self::Unknown(value.to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiState {
    pub active: SourceUiActiveState,
    pub visible: bool,
}

impl Default for SourceUiState {
    fn default() -> Self {
        Self {
            active: SourceUiActiveState::Normal,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceUiActiveState {
    Normal,
    Disabled,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiAspect {
    pub localization_id: Option<String>,
    pub authored_string: Option<String>,
    pub hit_policy: SourceUiHitPolicy,
    pub pad_x: Option<i32>,
    pub pad_y: Option<i32>,
    pub draw_3d: bool,
    pub auto_size: Option<bool>,
    pub default: Option<SourceUiVisual>,
    pub standard: Vec<SourceUiNamedVisual>,
    pub alternate: Vec<SourceUiNamedVisual>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
    pub unknown_elements: Vec<SourceUiPayloadNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceUiHitPolicy {
    Always,
    Never,
    Normal,
    Region,
    Unknown(String),
}

impl SourceUiHitPolicy {
    pub fn parse(value: &str) -> Self {
        match value {
            "always" | "true" | "1" => Self::Always,
            "never" | "false" | "0" => Self::Never,
            "normal" => Self::Normal,
            "region" => Self::Region,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiNamedVisual {
    pub name: String,
    pub visual: SourceUiVisual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiVisual {
    pub image: Option<AssetPath>,
    pub sound: Option<String>,
    pub hit_policy: Option<SourceUiHitPolicy>,
    pub rect: Option<SourceUiMetricRect>,
    pub color: Option<SourceUiColor>,
    pub font: Option<SourceUiFont>,
    pub text_alignment: Option<String>,
    /// Closed authored text layout policy (`multi` in shipped UI).
    pub text_format: Option<String>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
    pub unknown_elements: Vec<SourceUiPayloadNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiMetricRect {
    pub x: SourceUiMetric,
    pub y: SourceUiMetric,
    pub width: SourceUiMetric,
    pub height: SourceUiMetric,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceUiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceUiColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiFont {
    pub face: Option<String>,
    pub size: Option<f32>,
    pub align: Option<String>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub shadow_x: Option<i32>,
    pub shadow_y: Option<i32>,
    /// Presence is retained so a named visual can inherit an omitted value
    /// from its `default` BFFont while still being able to author `false`.
    pub bold: Option<bool>,
    pub underline: Option<bool>,
    pub color: Option<SourceUiColor>,
    pub image: Option<AssetPath>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiShowHideAnimation {
    pub seconds: Option<f32>,
    pub exit_rate: Option<f32>,
    pub initial_seconds: Option<f32>,
    pub initial_direction: Option<bool>,
    pub bob_seconds: Option<f32>,
    pub delay_seconds: Option<f32>,
    pub function: Option<String>,
    pub start: Option<SourceUiRect>,
    pub end: Option<SourceUiRect>,
    pub colors: Option<SourceUiShowHideColors>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiShowHideColors {
    pub affects_text: Option<bool>,
    pub start: Option<SourceUiColor>,
    pub end: Option<SourceUiColor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiHelp {
    pub ids: Option<String>,
    pub name: Option<String>,
    pub short: Option<String>,
    pub long: Option<String>,
    pub help: Option<String>,
    pub lower: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiField {
    pub name: Option<String>,
    pub type_name: Option<String>,
    pub format: Option<String>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiFinanceCategory {
    pub measurement: String,
    pub name: String,
    pub format: Option<AssetPath>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiEventBlock {
    pub trigger: SourceUiEventTrigger,
    pub events: Vec<SourceUiEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiNamedEventList {
    pub name: String,
    pub events: Vec<SourceUiEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceUiEventTrigger {
    Enter,
    Leave,
    Activate,
    DoubleClick,
    AnimationCompleted,
    Show,
    Hide,
    On,
    Off,
    TextChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiEvent {
    pub message: String,
    pub data: Option<String>,
    pub string: Option<String>,
    pub value: Option<String>,
    pub key: Option<String>,
    pub val: Option<String>,
    pub event_type: Option<String>,
    pub target_child: Option<String>,
    pub color: Option<SourceUiColor>,
    pub rect: [Option<String>; 4],
    pub xml_object: Option<SourceUiXmlObjectEvent>,
    pub child: Option<Box<SourceUiEvent>>,
    pub payload: Vec<SourceUiPayloadNode>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
}

impl SourceUiEvent {
    /// `UI_CHILD` and `UI_ROOT_CHILD` send their child message to the named node.
    /// The child carries that target wherever it is lowered, including a dialog's
    /// Escape binding included from the hotkey file.
    pub fn set_child(&mut self, mut child: SourceUiEvent) {
        if matches!(self.message.as_str(), "UI_CHILD" | "UI_ROOT_CHILD")
            && child.target_child.is_none()
        {
            child.target_child.clone_from(&self.target_child);
        }
        self.child = Some(Box::new(child));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiXmlObjectEvent {
    pub message_type: Option<String>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub payload: SourceUiPayloadNode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiHotkeyModeReference {
    pub file: AssetPath,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiHotkey {
    pub trigger: SourceUiHotkeyTrigger,
    pub localization_id: Option<String>,
    pub character: Option<String>,
    pub code: Option<u32>,
    pub control_state: Option<String>,
    pub allow_repeat: Option<bool>,
    pub file: Option<AssetPath>,
    pub node: Option<String>,
    pub event: SourceUiEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceUiHotkeyTrigger {
    Down,
    Up,
    /// `<char>`: `code` is a character code, so Ctrl-M is 13 rather than M.
    Character,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiAttribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiPayloadNode {
    pub name: String,
    pub attributes: Vec<SourceUiAttribute>,
    pub children: Vec<SourceUiPayloadNode>,
    pub text: Option<String>,
    pub span: OrderedSourceDocumentSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiSkin {
    pub name: String,
    pub replacement_type: Option<String>,
    pub directory: Option<AssetPath>,
    pub replacements: Vec<SourceUiSkinReplacement>,
    pub themes: Vec<SourceUiSkinTheme>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiSkinTheme {
    pub name: String,
    pub replacements: Vec<SourceUiSkinReplacement>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiSkinReplacement {
    pub target_name: String,
    /// Replacement image. File skins can omit this and use their skin directory.
    pub image: Option<AssetPath>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiGrid {
    pub auto_size: Option<bool>,
    pub auto_size_parent: Option<bool>,
    pub columns: Option<i32>,
    pub rows: Option<i32>,
    pub x_spacing: Option<i32>,
    pub y_spacing: Option<i32>,
    pub column_width: Option<i32>,
    pub row_height: Option<i32>,
    pub initial_x: Option<i32>,
    pub initial_y: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiButton {
    pub auto_size: Option<bool>,
    pub min_height: Option<i32>,
    pub toggle: Option<bool>,
    pub sticky: Option<bool>,
    pub repress: Option<bool>,
    pub activate_data: Option<String>,
    pub repeat_delay_seconds: Option<f32>,
    pub hold_change: Option<f32>,
    pub hold_interval_cap_seconds: Option<f32>,
    pub broadcast: Option<bool>,
    pub delayed_activation_seconds: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiList {
    pub grid: SourceUiGrid,
    pub row_template: Option<String>,
    pub balance_sheet_layout: Option<String>,
    pub count_component: Option<String>,
    pub update_seconds: Option<f32>,
    pub source: SourceUiListSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceUiListSource {
    #[default]
    Unbound,
    SelectedEntityInventory,
    ScenarioObjectives,
    ZoopediaTableOfContents,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiSlider {
    pub value_type: Option<String>,
    pub span: Option<f32>,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub increment: Option<f32>,
    pub initial_value: Option<f32>,
    pub thumb_name: Option<String>,
    pub thumb_region: Option<SourceUiRegion>,
    pub axis: Option<SourceUiDragAxis>,
    pub style: Option<String>,
    pub field: Option<String>,
    pub on_change: Option<String>,
    pub minimum_thumb_size: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiDrag {
    pub minimum_width: Option<i32>,
    pub maximum_width: Option<i32>,
    pub minimum_height: Option<i32>,
    pub maximum_height: Option<i32>,
    pub bounded: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiDragCommand {
    pub message: Option<String>,
    pub axis: SourceUiDragAxis,
    pub flip: Option<String>,
    pub data: Option<String>,
    pub string: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SourceUiDragAxis {
    X,
    Y,
    #[default]
    Both,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiGraph {
    pub minimum: Option<f32>,
    pub maximum: Option<f32>,
    pub graph_type: Option<String>,
    pub x_label_count: Option<i32>,
    pub y_label_count: Option<i32>,
    pub forced_minimum_y: Option<f32>,
    pub forced_maximum_y: Option<f32>,
    pub force_y_values: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiImageEntry {
    pub key: Option<String>,
    pub localization_id: Option<String>,
    pub image: Option<AssetPath>,
    pub rect: Option<SourceUiMetricRect>,
    pub unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiMapLayer {
    pub kind: String,
    pub node: Option<String>,
    pub icon: Option<AssetPath>,
    pub localization_id: Option<String>,
    pub source: Option<String>,
    pub has_canvas: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUiWorldMap {
    pub layers: Vec<SourceUiMapLayer>,
    pub colors: SourceUiMapColors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceUiMapColors {
    pub terrain: SourceUiColor,
    pub fence: SourceUiColor,
    pub curb: SourceUiColor,
    pub zoo_wall: SourceUiColor,
    pub path: SourceUiColor,
    pub elevated_path: SourceUiColor,
    pub ground_track: SourceUiColor,
    pub sky_track: SourceUiColor,
    pub water: SourceUiColor,
}

impl Default for SourceUiMapColors {
    fn default() -> Self {
        Self {
            terrain: SourceUiColor {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
            fence: SourceUiColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
            curb: SourceUiColor {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            zoo_wall: SourceUiColor {
                red: 0,
                green: 100,
                blue: 0,
                alpha: 255,
            },
            path: SourceUiColor {
                red: 128,
                green: 128,
                blue: 128,
                alpha: 255,
            },
            elevated_path: SourceUiColor {
                red: 0,
                green: 255,
                blue: 255,
                alpha: 128,
            },
            ground_track: SourceUiColor {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 128,
            },
            sky_track: SourceUiColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 128,
            },
            water: SourceUiColor {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiText {
    pub auto_size: Option<bool>,
    pub min_height: Option<i32>,
    pub text_type: Option<String>,
    pub text_format: Option<String>,
    pub localization_id: Option<String>,
    pub authored_string: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiTextEdit {
    pub maximum_length: Option<i32>,
    pub change_sound: Option<String>,
    pub error_sound: Option<String>,
    pub legal_filename_only: Option<bool>,
    pub highlight: Option<bool>,
    pub cursor_on_seconds: Option<f32>,
    pub cursor_off_seconds: Option<f32>,
    pub changed_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiGlobe {
    pub primary_model: Option<AssetPath>,
    pub primary_translation: Option<[f32; 3]>,
    pub clouds_model: Option<AssetPath>,
    pub clouds_translation: Option<[f32; 3]>,
    pub dot_model: Option<AssetPath>,
    pub dot_translation: Option<[f32; 3]>,
    pub selected_dot_model: Option<AssetPath>,
    pub selected_dot_translation: Option<[f32; 3]>,
    pub pointer_model: Option<AssetPath>,
    pub dot_highlight_cursor: Option<String>,
    pub secondary_models: Vec<(String, AssetPath)>,
    pub selection_rotate_speed: Option<f32>,
    pub mouse_increment: Option<f32>,
    pub mouse_down_friction: Option<f32>,
    pub mouse_up_friction: Option<f32>,
    pub friction_transition_seconds: Option<f32>,
    pub move_seconds: Option<f32>,
    pub scream_threshold: Option<f32>,
    pub scream_delay_seconds: Option<f32>,
    pub dot_highlight_color: Option<SourceUiColor>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiTooltip {
    pub text: SourceUiText,
    pub target: Option<String>,
    pub tooltip_type: Option<String>,
    pub floating: Option<bool>,
    pub autohide: Option<bool>,
    pub offset_x: Option<i32>,
    pub offset_y: Option<i32>,
    pub appear_seconds: Option<f32>,
    pub display_seconds: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiTreeElement {
    pub grid: SourceUiGrid,
    pub item_id: Option<String>,
    pub expanded: Option<bool>,
    pub selected: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceUiTimedEvent {
    pub delay_seconds: f32,
    pub event: SourceUiEvent,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiTypeList {
    pub grid: SourceUiGrid,
    pub root_type: Option<String>,
    pub root_types: Vec<String>,
    pub excluded_root_types: Vec<String>,
    pub non_remembered_root_types: Vec<String>,
    /// None inherits the template; an explicitly empty element clears it.
    pub filter_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiWindow {
    pub title: Option<String>,
    pub modal: Option<bool>,
    pub draggable: Option<bool>,
    pub horizontal: Option<bool>,
    pub vertical: Option<bool>,
    pub wheel_scroll: Option<bool>,
    pub horizontal_scroll: Option<String>,
    pub vertical_scroll: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceUiAnimation {
    pub seconds: Option<f32>,
    pub start: Option<SourceUiRect>,
    pub end: Option<SourceUiRect>,
    pub colors: Option<SourceUiShowHideColors>,
    pub exit_rate: Option<f32>,
    pub initial_time: Option<f32>,
    pub initial_direction: Option<bool>,
    pub bob_seconds: Option<f32>,
    pub delay_seconds: Option<f32>,
    pub function: Option<String>,
}
