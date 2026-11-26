pub mod authored_document_projection;
pub(crate) mod authored_hotkey_projection;
pub(super) mod authored_list_row_projection;
mod authored_node_action_routing_projection;
mod authored_node_layout_projection;
mod authored_node_picking_projection;
mod authored_node_property_binding_projection;
mod authored_node_visual_projection;
pub(super) mod authored_scenario_goal_projection;
mod authored_text_projection;
mod authored_widget_projection;
mod document_asset_readiness;

pub(super) mod ui_document_opening;

pub(super) fn register_document_attachment_cleanup(app: &mut bevy::prelude::App) {
    app.add_observer(authored_document_projection::despawn_document_attachment_to_missing_owner);
}
