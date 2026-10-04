//! Parsed UI XML/BFXML records.

use crate::assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentSpan, path::AssetPath,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiNode {
    pub(crate) kind: SourceUiWidgetKind,
    pub(crate) name: Option<String>,
    pub(crate) template: Option<String>,
    pub(crate) template_name: Option<String>,
    /// Lowered semantic for a scenario objective row variant.
    ///
    /// This field is introduced by UI source normalization when it lowers
    /// `ZTGoalPanel`; it never represents arbitrary original UI vocabulary.
    pub(crate) scenario_objective_visual: Option<String>,
    pub(crate) cursor: Option<String>,
    pub(crate) modal: Option<bool>,
    pub(crate) always_hit: Option<SourceUiHitPolicy>,
    pub(crate) cacheable_from_lua: Option<bool>,
    pub(crate) use_list_box_color_if_in_container: Option<bool>,
    /// Semantic clipping supplied by list controls, including through a
    /// template chain. This is derived from widget kind rather than guessed
    /// from a particular document or node name.
    pub(crate) clip_children: bool,
    /// Original expansion-availability guard. The source loader asks the
    /// application whether this numbered pack is available before constructing
    /// the widget.
    pub(crate) x_pack: Option<u32>,
    pub(crate) background_layout: Option<SourceUiBackgroundLayout>,
    pub(crate) region: Option<SourceUiRegion>,
    pub(crate) state: SourceUiState,
    pub(crate) aspect: Option<SourceUiAspect>,
    pub(crate) show_hide_animation: Option<SourceUiShowHideAnimation>,
    pub(crate) notify_animation_completed: Option<bool>,
    pub(crate) help: Option<SourceUiHelp>,
    pub(crate) text_format: Option<String>,
    pub(crate) widget: SourceUiWidgetData,
    pub(crate) fields: Vec<SourceUiField>,
    pub(crate) finance_categories: Vec<SourceUiFinanceCategory>,
    pub(crate) hotkeys: Vec<SourceUiHotkey>,
    /// `<UIHotKeys><file name node/>`: a named mode in a hotkey document whose
    /// bindings belong to this node.
    pub(crate) hotkey_mode_references: Vec<SourceUiHotkeyModeReference>,
    pub(crate) events: Vec<SourceUiEventBlock>,
    pub(crate) children: Vec<SourceUiNode>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
    pub(crate) unknown_elements: Vec<SourceUiPayloadNode>,
    pub(crate) span: OrderedSourceDocumentSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiBackgroundLayout {
    pub(crate) image: AssetPath,
    pub(crate) source: [i32; 4],
    pub(crate) padding: [i32; 2],
    pub(crate) width: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceUiWidgetKind {
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
    pub(crate) fn from_tag(tag: &str) -> Self {
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

    pub(crate) fn is_known_tag(tag: &str) -> bool {
        !matches!(Self::from_tag(tag), Self::Unknown(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SourceUiWidgetData {
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
pub(crate) struct SourceUiRegion {
    pub(crate) x: SourceUiMetric,
    pub(crate) y: SourceUiMetric,
    pub(crate) width: SourceUiMetric,
    pub(crate) height: SourceUiMetric,
    pub(crate) x_align: SourceUiAlignment,
    pub(crate) y_align: SourceUiAlignment,
    pub(crate) width_align: SourceUiAlignment,
    pub(crate) height_align: SourceUiAlignment,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SourceUiMetric {
    Number(f32),
    Token(String),
}

impl SourceUiMetric {
    pub(crate) fn parse(value: Option<&str>) -> Self {
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
pub(crate) enum SourceUiAlignment {
    Min,
    Max,
    Length,
    Mid,
    PercentMin,
    PercentMax,
    Unknown(String),
}

impl SourceUiAlignment {
    pub(crate) fn parse(value: Option<&str>, default: Self) -> Self {
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
pub(crate) struct SourceUiState {
    pub(crate) active: SourceUiActiveState,
    pub(crate) visible: bool,
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
pub(crate) enum SourceUiActiveState {
    Normal,
    Disabled,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiAspect {
    pub(crate) localization_id: Option<String>,
    pub(crate) authored_string: Option<String>,
    pub(crate) hit_policy: SourceUiHitPolicy,
    pub(crate) pad_x: Option<i32>,
    pub(crate) pad_y: Option<i32>,
    pub(crate) draw_3d: bool,
    pub(crate) auto_size: Option<bool>,
    pub(crate) default: Option<SourceUiVisual>,
    pub(crate) standard: Vec<SourceUiNamedVisual>,
    pub(crate) alternate: Vec<SourceUiNamedVisual>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
    pub(crate) unknown_elements: Vec<SourceUiPayloadNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceUiHitPolicy {
    Always,
    Never,
    Normal,
    Region,
    Unknown(String),
}

impl SourceUiHitPolicy {
    pub(crate) fn parse(value: &str) -> Self {
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
pub(crate) struct SourceUiNamedVisual {
    pub(crate) name: String,
    pub(crate) visual: SourceUiVisual,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiVisual {
    pub(crate) image: Option<AssetPath>,
    pub(crate) sound: Option<String>,
    pub(crate) hit_policy: Option<SourceUiHitPolicy>,
    pub(crate) rect: Option<SourceUiMetricRect>,
    pub(crate) color: Option<SourceUiColor>,
    pub(crate) font: Option<SourceUiFont>,
    pub(crate) text_alignment: Option<String>,
    /// Closed authored text layout policy (`multi` in shipped UI).
    pub(crate) text_format: Option<String>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
    pub(crate) unknown_elements: Vec<SourceUiPayloadNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiMetricRect {
    pub(crate) x: SourceUiMetric,
    pub(crate) y: SourceUiMetric,
    pub(crate) width: SourceUiMetric,
    pub(crate) height: SourceUiMetric,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SourceUiRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceUiColor {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
    pub(crate) alpha: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiFont {
    pub(crate) face: Option<String>,
    pub(crate) size: Option<f32>,
    pub(crate) align: Option<String>,
    pub(crate) x: Option<i32>,
    pub(crate) y: Option<i32>,
    pub(crate) shadow_x: Option<i32>,
    pub(crate) shadow_y: Option<i32>,
    /// Presence is retained so a named visual can inherit an omitted value
    /// from its `default` BFFont while still being able to author `false`.
    pub(crate) bold: Option<bool>,
    pub(crate) underline: Option<bool>,
    pub(crate) color: Option<SourceUiColor>,
    pub(crate) image: Option<AssetPath>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiShowHideAnimation {
    pub(crate) seconds: Option<f32>,
    pub(crate) exit_rate: Option<f32>,
    pub(crate) initial_seconds: Option<f32>,
    pub(crate) initial_direction: Option<bool>,
    pub(crate) bob_seconds: Option<f32>,
    pub(crate) delay_seconds: Option<f32>,
    pub(crate) function: Option<String>,
    pub(crate) start: Option<SourceUiRect>,
    pub(crate) end: Option<SourceUiRect>,
    pub(crate) colors: Option<SourceUiShowHideColors>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiShowHideColors {
    pub(crate) affects_text: Option<bool>,
    pub(crate) start: Option<SourceUiColor>,
    pub(crate) end: Option<SourceUiColor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiHelp {
    pub(crate) ids: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) short: Option<String>,
    pub(crate) long: Option<String>,
    pub(crate) help: Option<String>,
    pub(crate) lower: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiField {
    pub(crate) name: Option<String>,
    pub(crate) type_name: Option<String>,
    pub(crate) format: Option<String>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiFinanceCategory {
    pub(crate) measurement: String,
    pub(crate) name: String,
    pub(crate) format: Option<AssetPath>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiEventBlock {
    pub(crate) trigger: SourceUiEventTrigger,
    pub(crate) events: Vec<SourceUiEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiNamedEventList {
    pub(crate) name: String,
    pub(crate) events: Vec<SourceUiEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceUiEventTrigger {
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
pub(crate) struct SourceUiEvent {
    pub(crate) message: String,
    pub(crate) data: Option<String>,
    pub(crate) string: Option<String>,
    pub(crate) value: Option<String>,
    pub(crate) key: Option<String>,
    pub(crate) val: Option<String>,
    pub(crate) event_type: Option<String>,
    pub(crate) target_child: Option<String>,
    pub(crate) color: Option<SourceUiColor>,
    pub(crate) rect: [Option<String>; 4],
    pub(crate) xml_object: Option<SourceUiXmlObjectEvent>,
    pub(crate) child: Option<Box<SourceUiEvent>>,
    pub(crate) payload: Vec<SourceUiPayloadNode>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
}

impl SourceUiEvent {
    /// `UI_CHILD` sends its child message to the named node, so the child
    /// carries that target wherever it is lowered, including a dialog's
    /// Escape binding included from the hotkey file.
    pub(crate) fn set_child(&mut self, mut child: SourceUiEvent) {
        if self.message == "UI_CHILD" && child.target_child.is_none() {
            child.target_child.clone_from(&self.target_child);
        }
        self.child = Some(Box::new(child));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiXmlObjectEvent {
    pub(crate) message_type: Option<String>,
    pub(crate) key: Option<String>,
    pub(crate) value: Option<String>,
    pub(crate) payload: SourceUiPayloadNode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiHotkeyModeReference {
    pub(crate) file: AssetPath,
    pub(crate) mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiHotkey {
    pub(crate) trigger: SourceUiHotkeyTrigger,
    pub(crate) localization_id: Option<String>,
    pub(crate) character: Option<String>,
    pub(crate) code: Option<u32>,
    pub(crate) control_state: Option<String>,
    pub(crate) allow_repeat: Option<bool>,
    pub(crate) file: Option<AssetPath>,
    pub(crate) node: Option<String>,
    pub(crate) event: SourceUiEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceUiHotkeyTrigger {
    Down,
    Up,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiAttribute {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiPayloadNode {
    pub(crate) name: String,
    pub(crate) attributes: Vec<SourceUiAttribute>,
    pub(crate) children: Vec<SourceUiPayloadNode>,
    pub(crate) text: Option<String>,
    pub(crate) span: OrderedSourceDocumentSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiSkin {
    pub(crate) name: String,
    pub(crate) replacement_type: Option<String>,
    pub(crate) directory: Option<AssetPath>,
    pub(crate) replacements: Vec<SourceUiSkinReplacement>,
    pub(crate) themes: Vec<SourceUiSkinTheme>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiSkinTheme {
    pub(crate) name: String,
    pub(crate) replacements: Vec<SourceUiSkinReplacement>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiSkinReplacement {
    pub(crate) target_name: String,
    /// Replacement image. File skins can omit this and use their skin directory.
    pub(crate) image: Option<AssetPath>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiGrid {
    pub(crate) auto_size: Option<bool>,
    pub(crate) auto_size_parent: Option<bool>,
    pub(crate) columns: Option<i32>,
    pub(crate) rows: Option<i32>,
    pub(crate) x_spacing: Option<i32>,
    pub(crate) y_spacing: Option<i32>,
    pub(crate) column_width: Option<i32>,
    pub(crate) row_height: Option<i32>,
    pub(crate) initial_x: Option<i32>,
    pub(crate) initial_y: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiButton {
    pub(crate) auto_size: Option<bool>,
    pub(crate) min_height: Option<i32>,
    pub(crate) toggle: Option<bool>,
    pub(crate) sticky: Option<bool>,
    pub(crate) repress: Option<bool>,
    pub(crate) activate_data: Option<String>,
    pub(crate) repeat_delay_seconds: Option<f32>,
    pub(crate) hold_change: Option<f32>,
    pub(crate) hold_interval_cap_seconds: Option<f32>,
    pub(crate) broadcast: Option<bool>,
    pub(crate) delayed_activation_seconds: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiList {
    pub(crate) grid: SourceUiGrid,
    pub(crate) row_template: Option<String>,
    pub(crate) balance_sheet_layout: Option<String>,
    pub(crate) count_component: Option<String>,
    pub(crate) update_seconds: Option<f32>,
    pub(crate) source: SourceUiListSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SourceUiListSource {
    #[default]
    Unbound,
    SelectedEntityInventory,
    ScenarioObjectives,
    ZoopediaTableOfContents,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiSlider {
    pub(crate) value_type: Option<String>,
    pub(crate) span: Option<f32>,
    pub(crate) min: Option<f32>,
    pub(crate) max: Option<f32>,
    pub(crate) increment: Option<f32>,
    pub(crate) initial_value: Option<f32>,
    pub(crate) thumb_name: Option<String>,
    pub(crate) thumb_region: Option<SourceUiRegion>,
    pub(crate) axis: Option<SourceUiDragAxis>,
    pub(crate) style: Option<String>,
    pub(crate) field: Option<String>,
    pub(crate) on_change: Option<String>,
    pub(crate) minimum_thumb_size: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiDrag {
    pub(crate) minimum_width: Option<i32>,
    pub(crate) maximum_width: Option<i32>,
    pub(crate) minimum_height: Option<i32>,
    pub(crate) maximum_height: Option<i32>,
    pub(crate) bounded: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiDragCommand {
    pub(crate) message: Option<String>,
    pub(crate) axis: SourceUiDragAxis,
    pub(crate) flip: Option<String>,
    pub(crate) data: Option<String>,
    pub(crate) string: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum SourceUiDragAxis {
    X,
    Y,
    #[default]
    Both,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiGraph {
    pub(crate) minimum: Option<f32>,
    pub(crate) maximum: Option<f32>,
    pub(crate) graph_type: Option<String>,
    pub(crate) x_label_count: Option<i32>,
    pub(crate) y_label_count: Option<i32>,
    pub(crate) forced_minimum_y: Option<f32>,
    pub(crate) forced_maximum_y: Option<f32>,
    pub(crate) force_y_values: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiImageEntry {
    pub(crate) key: Option<String>,
    pub(crate) localization_id: Option<String>,
    pub(crate) image: Option<AssetPath>,
    pub(crate) rect: Option<SourceUiMetricRect>,
    pub(crate) unknown_attributes: Vec<SourceUiAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiMapLayer {
    pub(crate) kind: String,
    pub(crate) node: Option<String>,
    pub(crate) icon: Option<AssetPath>,
    pub(crate) localization_id: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) has_canvas: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceUiWorldMap {
    pub(crate) layers: Vec<SourceUiMapLayer>,
    pub(crate) colors: SourceUiMapColors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceUiMapColors {
    pub(crate) terrain: SourceUiColor,
    pub(crate) fence: SourceUiColor,
    pub(crate) curb: SourceUiColor,
    pub(crate) zoo_wall: SourceUiColor,
    pub(crate) path: SourceUiColor,
    pub(crate) elevated_path: SourceUiColor,
    pub(crate) ground_track: SourceUiColor,
    pub(crate) sky_track: SourceUiColor,
    pub(crate) water: SourceUiColor,
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
pub(crate) struct SourceUiText {
    pub(crate) auto_size: Option<bool>,
    pub(crate) min_height: Option<i32>,
    pub(crate) text_type: Option<String>,
    pub(crate) text_format: Option<String>,
    pub(crate) localization_id: Option<String>,
    pub(crate) authored_string: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiTextEdit {
    pub(crate) maximum_length: Option<i32>,
    pub(crate) change_sound: Option<String>,
    pub(crate) error_sound: Option<String>,
    pub(crate) legal_filename_only: Option<bool>,
    pub(crate) highlight: Option<bool>,
    pub(crate) cursor_on_seconds: Option<f32>,
    pub(crate) cursor_off_seconds: Option<f32>,
    pub(crate) changed_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiGlobe {
    pub(crate) primary_model: Option<AssetPath>,
    pub(crate) primary_translation: Option<[f32; 3]>,
    pub(crate) clouds_model: Option<AssetPath>,
    pub(crate) clouds_translation: Option<[f32; 3]>,
    pub(crate) dot_model: Option<AssetPath>,
    pub(crate) dot_translation: Option<[f32; 3]>,
    pub(crate) selected_dot_model: Option<AssetPath>,
    pub(crate) selected_dot_translation: Option<[f32; 3]>,
    pub(crate) pointer_model: Option<AssetPath>,
    pub(crate) dot_highlight_cursor: Option<String>,
    pub(crate) secondary_models: Vec<(String, AssetPath)>,
    pub(crate) selection_rotate_speed: Option<f32>,
    pub(crate) mouse_increment: Option<f32>,
    pub(crate) mouse_down_friction: Option<f32>,
    pub(crate) mouse_up_friction: Option<f32>,
    pub(crate) friction_transition_seconds: Option<f32>,
    pub(crate) move_seconds: Option<f32>,
    pub(crate) scream_threshold: Option<f32>,
    pub(crate) scream_delay_seconds: Option<f32>,
    pub(crate) dot_highlight_color: Option<SourceUiColor>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiTooltip {
    pub(crate) text: SourceUiText,
    pub(crate) target: Option<String>,
    pub(crate) tooltip_type: Option<String>,
    pub(crate) floating: Option<bool>,
    pub(crate) autohide: Option<bool>,
    pub(crate) offset_x: Option<i32>,
    pub(crate) offset_y: Option<i32>,
    pub(crate) appear_seconds: Option<f32>,
    pub(crate) display_seconds: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiTreeElement {
    pub(crate) grid: SourceUiGrid,
    pub(crate) item_id: Option<String>,
    pub(crate) expanded: Option<bool>,
    pub(crate) selected: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceUiTimedEvent {
    pub(crate) delay_seconds: f32,
    pub(crate) event: SourceUiEvent,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiTypeList {
    pub(crate) grid: SourceUiGrid,
    pub(crate) root_type: Option<String>,
    pub(crate) root_types: Vec<String>,
    pub(crate) excluded_root_types: Vec<String>,
    pub(crate) non_remembered_root_types: Vec<String>,
    /// None inherits the template; an explicitly empty element clears it.
    pub(crate) filter_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiWindow {
    pub(crate) title: Option<String>,
    pub(crate) modal: Option<bool>,
    pub(crate) draggable: Option<bool>,
    pub(crate) horizontal: Option<bool>,
    pub(crate) vertical: Option<bool>,
    pub(crate) wheel_scroll: Option<bool>,
    pub(crate) horizontal_scroll: Option<String>,
    pub(crate) vertical_scroll: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceUiAnimation {
    pub(crate) seconds: Option<f32>,
    pub(crate) start: Option<SourceUiRect>,
    pub(crate) end: Option<SourceUiRect>,
    pub(crate) colors: Option<SourceUiShowHideColors>,
    pub(crate) exit_rate: Option<f32>,
    pub(crate) initial_time: Option<f32>,
    pub(crate) initial_direction: Option<bool>,
    pub(crate) bob_seconds: Option<f32>,
    pub(crate) delay_seconds: Option<f32>,
    pub(crate) function: Option<String>,
}
