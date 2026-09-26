use super::{
    action::{scenarios::ObjectiveStatusFilter, UiActionRecord},
    finance_table::UiFinanceTableCategoryDefinition,
    hotkey::UiDocumentHotkeyDefinition,
    node_layout::UiNodeRegionDefinition,
    node_presentation::{UiNodeAnimationDefinition, UiNodeVisualStateDefinition},
    node_property_binding::UiNodePropertyBinding,
    node_style::UiStyleRecord,
    widget::UiWidgetRecord,
};
use crate::AssetId;
use serde::{Deserialize, Serialize};

mod node_flag_operations;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct UiNodeFlags(pub u16);

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct UiNodeDefinition {
    pub id: AssetId,
    pub name: String,
    pub kind: UiNodeKind,
    pub parent: u32,
    pub children: Vec<u32>,
    pub rect: [f32; 4],
    pub anchors: [f32; 4],
    pub region: UiNodeRegionDefinition,
    pub text: String,
    pub cursor: AssetId,
    pub text_format: AssetId,
    pub help: UiNodeHelpDefinition,
    pub cacheable_from_lua: bool,
    pub field_definitions: Vec<UiNodeFieldDefinition>,
    pub finance_categories: Vec<UiFinanceTableCategoryDefinition>,
    pub style: UiStyleRecord,
    pub bindings: Vec<UiNodePropertyBinding>,
    pub actions: Vec<UiActionRecord>,
    pub animation: Option<UiNodeAnimationDefinition>,
    pub visuals: Vec<UiNodeVisualStateDefinition>,
    pub hotkeys: Vec<UiDocumentHotkeyDefinition>,
    pub widget: UiWidgetRecord,
    pub scenario_objective_visual: Option<ObjectiveStatusFilter>,
    pub flags: UiNodeFlags,
    pub z: i16,
    pub tab_order: i16,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiNodeHelpDefinition {
    pub name: AssetId,
    pub short: AssetId,
    pub long: AssetId,
    pub help: AssetId,
    pub lower: AssetId,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiNodeFieldDefinition {
    pub name: AssetId,
    pub value_type: AssetId,
    pub format: AssetId,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiNodeKind {
    Root,
    Container,
    Image,
    Text,
    Button,
    Toggle,
    Slider,
    Scroll,
    List,
    Edit,
    Animation,
    App,
    CompositeButton,
    Drag,
    DragCommand,
    DropList,
    Graph,
    MultiIcon,
    WorldMap,
    SoundManager,
    TextBuffer,
    ToggleSet,
    Globe,
    Tool,
    Tooltip,
    TreeElement,
    TimedPresentation,
    ContextList,
    FullscreenButton,
    GoalPanel,
    PhotoAlbum,
    TypeList,
    Window,
    XmlEdit,
}
