pub mod authored_button_runtime_policy;
pub mod authored_online_message_node_roles;
pub mod authored_reusable_list_and_table_runtime_types;
pub mod authored_ui_action_projection_components;
pub mod authored_ui_activation_contracts;
pub mod authored_ui_focus_state;
pub mod authored_ui_integer_range_components;
pub mod authored_ui_interaction_enabled_state;
pub mod authored_ui_node_projection_components;
pub mod authored_ui_selection_state;
pub mod authored_ui_visual_types;
pub mod input_frame_capture_types;
pub mod notification_contracts;
pub mod pointer_capture_types;
pub mod ui_document_asset_load_failure;
pub mod ui_document_lifecycle_contracts;

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiSet {
    Projection,
    DomainProjection,
    Interaction,
    Routing,
    Commands,
    Behavior,
    Notifications,
}
