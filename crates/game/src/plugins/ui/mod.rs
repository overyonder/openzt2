mod authored_globe_camera_framing;
mod authored_globe_interaction;
mod authored_globe_marker_presentation;
mod authored_globe_rotation_geometry;
use openzt2_game_data::ui_document::document::UiDocumentRole;
pub(crate) mod active_authored_ui_context;
pub(crate) mod animation;
mod authored_button_attention_pulse_presentation;
pub(crate) mod authored_button_runtime_policy;
mod authored_composite_button_activation_forwarding;
mod authored_credits_sequence_presentation;
mod authored_drag_gesture;
pub(crate) mod authored_globe_presentation_types;
mod authored_globe_scene_presentation;
mod authored_grid_layout_projection;
mod authored_hotkey_keyboard_activation;
pub(crate) mod authored_image_selection_diagnostic_override;
pub(crate) mod authored_modal_presentation;
pub(crate) mod authored_multi_icon_presentation;
pub(crate) mod authored_online_message_node_roles;
mod authored_rail_camera_presentation;
pub(crate) mod authored_reusable_list_and_table_runtime_types;
pub(crate) mod authored_text_edit_bevy_adaptation;
mod authored_text_physical_glyph_projection;
mod authored_timed_action_sequence;
pub(crate) mod authored_toggle_selection_transitions;
pub(crate) mod authored_tooltip_presentation;
pub(crate) mod authored_tree_expansion_and_row_indentation;
pub(crate) mod authored_ui_action_projection_components;
pub(crate) mod authored_ui_activation_contracts;
mod authored_ui_canvas_scaling_and_clipping;
pub(crate) mod authored_ui_change_activation_dispatch;
mod authored_ui_focus_navigation_and_activation;
pub(crate) mod authored_ui_focus_state;
pub(crate) mod authored_ui_image_content_binding;
pub(crate) mod authored_ui_integer_range_components;
pub(crate) mod authored_ui_interaction_enabled_binding;
pub(crate) mod authored_ui_interaction_enabled_state;
mod authored_ui_interaction_enabled_state_application;
mod authored_ui_interaction_visual_presentation;
pub(crate) mod authored_ui_layout_participation;
pub(crate) mod authored_ui_node_projection_components;
pub(crate) mod authored_ui_pointer_activation;
pub(crate) mod authored_ui_presentation_action_application;
pub(crate) mod authored_ui_presentation_slot_bindings;
pub(crate) mod authored_ui_selection_binding;
pub(crate) mod authored_ui_selection_state;
mod authored_ui_source_rectangle_presentation;
pub(crate) mod authored_ui_text_content_binding;
pub(crate) mod authored_ui_text_layout_presentation;
pub(crate) mod authored_ui_visual_types;
mod authored_visual_layer_draw_order;
mod authored_wheel_scroll;
mod authored_window_interaction;
mod controller_ui_action_routing;
mod cross_document_node_visibility;
pub(crate) mod cursor;
mod drop_list;
mod focused_or_selected_node_scroll_reveal;
mod full_canvas_click_catcher;
mod graph_generated_geometry;
mod graph_history_series_rendering;
mod graph_presentation;
mod graph_presentation_types;
mod held_button_repeat;
mod localized_countdown_presentation;
pub(crate) mod localized_ui_text_writing;
mod main_toolbar;
mod notification;
pub(crate) mod notification_contracts;
pub(crate) mod picking;
pub(crate) mod projection;
pub(crate) mod scenario_objective_status_visual_classification;
pub(crate) mod scrollbar_value_and_scroll_position_synchronization;
pub(crate) mod slider;
pub(crate) mod ui_document_asset_load_failure;
pub(crate) mod ui_document_lifecycle_contracts;

use self::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use self::cursor::SetWaitCursor;
use self::scrollbar_value_and_scroll_position_synchronization as scrollbar_synchronization;

use bevy::prelude::*;

use self::ui_document_lifecycle_contracts::{
    HideUiDocument, ShowUiDocument, ShowUiRole, UiRoleRequests,
};
use crate::application_lifecycle::GamePhase;
use crate::application_schedule::GameSet;

pub struct GameUiPlugin;

#[derive(Component)]
struct InGameUiOwner;

#[derive(Component)]
struct ApplicationUiOwner;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum UiSet {
    Projection,
    DomainProjection,
    Interaction,
    Routing,
    Commands,
    Behavior,
    Notifications,
}

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        cursor::initialize(app);
        picking::initialize(app);
        app.add_systems(PostUpdate,
            authored_ui_source_rectangle_presentation::resolve_authored_image_source_rectangles_after_image_changes
                .before(bevy::ui::UiSystems::Layout));
        app.add_message::<ShowUiDocument>()
            .add_message::<SetWaitCursor>()
            .add_message::<SetUiListRowCount>()
            .add_message::<ShowUiRole>()
            .add_message::<HideUiDocument>()
            .add_message::<authored_ui_interaction_enabled_state_application::SetAuthoredUiNodeInteractionEnabled>()
            .add_message::<authored_ui_activation_contracts::UiNodeActivated>()
            .add_message::<authored_ui_presentation_action_application::RequestUiPresentationAction>()
            .add_message::<animation::UiAnimationCompleted>()
            .add_message::<notification_contracts::ShowNotification>()
            .add_message::<notification_contracts::NotificationPresented>()
            .add_message::<notification_contracts::NotificationRejected>()
            .add_message::<ui_document_asset_load_failure::UiDocumentAssetLoadFailed>()
            .init_resource::<UiRoleRequests>()
            .add_observer(authored_text_edit_bevy_adaptation::dispatch_authored_change_activation_after_bevy_text_edit)
            .add_systems(
                First,
                authored_text_edit_bevy_adaptation::restore_authored_text_edit_scroll_before_input,
            )
            .add_systems(
                Startup,
                (
                    authored_ui_text_layout_presentation::bind_authored_windows_font_roles_to_available_platform_font_families,
                    show_application_ui,
                ),
            )
            .add_systems(OnEnter(GamePhase::InGame), show_in_game_hud)
            .add_systems(OnExit(GamePhase::InGame), hide_in_game_hud)
            .configure_sets(
                Update,
                (
                    UiSet::Projection,
                    UiSet::DomainProjection,
                    UiSet::Interaction,
                    UiSet::Routing,
                    UiSet::Commands,
                    UiSet::Behavior,
                    UiSet::Notifications,
                )
                    .chain()
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                Update,
                (
                    projection::hide_ui_documents,
                    projection::resolve_ui_roles,
                    projection::project_ui_documents
                        .after(projection::hide_ui_documents)
                        .after(projection::resolve_ui_roles),
                    projection::cleanup_orphaned_ui_documents
                        .after(projection::project_ui_documents),
                    graph_presentation::clip_authored_graph_generated_geometry
                        .after(projection::project_ui_documents),
                    main_toolbar::initialize_main_toolbar.after(projection::project_ui_documents),
                    authored_text_edit_bevy_adaptation::initialize_authored_text_edits_as_bevy_editable_text
                        .after(projection::project_ui_documents),
                    projection::reconcile_ui_list_rows,
                    authored_ui_canvas_scaling_and_clipping::scale_authored_ui_canvas_to_primary_window,
                    authored_ui_text_layout_presentation::align_authored_single_line_text_from_measured_glyph_run
                        .after(projection::project_ui_documents),
                )
                    .in_set(UiSet::Projection),
            )
            .add_systems(
                Update,
                cross_document_node_visibility::apply_pending_cross_document_node_visibility
                    .in_set(UiSet::DomainProjection),
            )
            .add_systems(
                Update,
                (
                    (
                        authored_hotkey_keyboard_activation::activate_projected_authored_hotkeys_from_keyboard_input,
                        authored_ui_interaction_enabled_state_application::apply_authored_ui_node_interaction_enabled_state_changes,
                        authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions,
                        authored_globe_interaction::interact_with_globe
                            .before(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes),
                        authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes,
                        authored_composite_button_activation_forwarding::forward_authored_composite_button_activations_to_projected_child_controls
                            .after(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes),
                        drop_list::toggle_or_close_authored_drop_list_displays_from_activations
                            .after(authored_composite_button_activation_forwarding::forward_authored_composite_button_activations_to_projected_child_controls),
                        cursor::apply_wait_cursor_requests,
                        cursor::present_cursor
                            .after(cursor::apply_wait_cursor_requests)
                            .after(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes),
                        full_canvas_click_catcher::hide_activated_or_elapsed_full_canvas_click_catchers
                            .after(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes),
                        slider::apply_slider_bounds,
                        slider::adjust_focused_sliders
                            .after(authored_ui_interaction_enabled_state_application::apply_authored_ui_node_interaction_enabled_state_changes)
                            .before(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions),
                        slider::adjust_sliders_from_pointer
                            .after(authored_ui_interaction_enabled_state_application::apply_authored_ui_node_interaction_enabled_state_changes),
                        (
                            authored_text_edit_bevy_adaptation::release_text_focus_after_pointer_press_elsewhere,
                            authored_text_edit_bevy_adaptation::synchronize_document_focus_into_bevy_text_input_focus,
                        ).chain()
                            .after(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes)
                            .after(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions)
                            .after(authored_ui_interaction_enabled_state_application::apply_authored_ui_node_interaction_enabled_state_changes),
                        authored_text_edit_bevy_adaptation::discard_pending_edits_and_focus_from_disabled_text_edits
                            .after(authored_ui_interaction_enabled_state_application::apply_authored_ui_node_interaction_enabled_state_changes),
                    ),
                    (
                        controller_ui_action_routing::route_controller_authored_actions,
                        authored_ui_focus_navigation_and_activation::activate_focused_authored_ui_node_from_confirm_or_cancel
                            .after(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions),
                        authored_ui_focus_navigation_and_activation::present_controller_authored_ui_focus
                            .after(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions)
                            .after(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes),
                        authored_ui_interaction_visual_presentation::present_authored_ui_interaction_visual_state
                            .after(authored_ui_interaction_enabled_state_application::apply_authored_ui_node_interaction_enabled_state_changes)
                            .after(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes)
                            .after(authored_ui_focus_navigation_and_activation::present_controller_authored_ui_focus)
                            .after(authored_toggle_selection_transitions::apply_authored_standalone_toggle_selection_transitions_from_presses)
                            .after(authored_toggle_selection_transitions::apply_authored_toggle_group_selection_transitions_from_presses),
                        authored_toggle_selection_transitions::apply_authored_standalone_toggle_selection_transitions_from_presses
                            .after(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes),
                        authored_toggle_selection_transitions::apply_authored_toggle_group_selection_transitions_from_presses
                            .after(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes),
                        authored_tree_expansion_and_row_indentation::toggle_authored_tree_expansion_from_presses
                            .after(authored_ui_pointer_activation::activate_authored_ui_nodes_from_pointer_interaction_changes),
                        authored_tree_expansion_and_row_indentation::present_authored_tree_children_from_expansion
                            .after(authored_tree_expansion_and_row_indentation::toggle_authored_tree_expansion_from_presses),
                        authored_tree_expansion_and_row_indentation::project_authored_tree_depth_into_containing_row_indentation,
                        authored_ui_change_activation_dispatch::dispatch_authored_focus_change_activations,
                        authored_ui_change_activation_dispatch::dispatch_authored_value_change_activations
                            .after(slider::apply_slider_bounds)
                            .after(slider::adjust_focused_sliders)
                            .after(slider::adjust_sliders_from_pointer),
                        authored_ui_change_activation_dispatch::dispatch_authored_visibility_change_activations,
                        authored_ui_change_activation_dispatch::dispatch_authored_selection_change_activations
                            .after(authored_toggle_selection_transitions::apply_authored_standalone_toggle_selection_transitions_from_presses)
                            .after(authored_toggle_selection_transitions::apply_authored_toggle_group_selection_transitions_from_presses),
                    ),
                )
                    .in_set(UiSet::Interaction),
            )
            .add_systems(
                Update,
                (
                    animation::advance_ui_animations,
                    animation::dispatch_ui_animation_commands
                        .after(animation::advance_ui_animations),
                    authored_ui_presentation_action_application::apply_authored_ui_presentation_actions_to_projected_bevy_tree,
                    authored_ui_presentation_action_application::activate_pending_selected_children_after_ui_document_projection
                        .after(authored_ui_presentation_action_application::apply_authored_ui_presentation_actions_to_projected_bevy_tree),
                    scrollbar_synchronization::apply_changed_authored_scrollbar_values_to_owned_bevy_scroll_positions
                        .after(authored_ui_presentation_action_application::apply_authored_ui_presentation_actions_to_projected_bevy_tree),
                    authored_timed_action_sequence::advance_authored_timed_action_sequences_and_activate_due_proxies,
                    localized_countdown_presentation::advance_localized_countdown_presentations_and_activate_completion_targets,
                )
                    .in_set(UiSet::Commands),
            )
            .add_systems(
                Update,
                (
                    (
                        graph_presentation::regenerate_visible_authored_graph_presentations_from_canonical_histories,
                        held_button_repeat::repeat_held_buttons,
                        authored_wheel_scroll::scroll_hovered_authored_lists_or_wheel_scroll_windows_from_mouse_wheel,
                        scrollbar_synchronization::project_owned_bevy_scroll_positions_into_authored_scrollbar_values
                            .after(authored_wheel_scroll::scroll_hovered_authored_lists_or_wheel_scroll_windows_from_mouse_wheel),
                        slider::present_slider_values.after(scrollbar_synchronization::project_owned_bevy_scroll_positions_into_authored_scrollbar_values),
                        slider::present_slider_value_labels.after(scrollbar_synchronization::project_owned_bevy_scroll_positions_into_authored_scrollbar_values),
                        scrollbar_synchronization::present_authored_scrollbar_decorations_from_owned_bevy_overflow
                            .after(projection::project_ui_documents)
                            .after(scrollbar_synchronization::project_owned_bevy_scroll_positions_into_authored_scrollbar_values),
                        authored_window_interaction::drag_authored_windows_from_accumulated_pointer_motion,
                        authored_drag_gesture::apply_authored_drag_gestures_to_bevy_layout_or_scroll_position,
                        authored_tooltip_presentation::update_authored_tooltip_presentations,
                        authored_multi_icon_presentation::present_multi_icons
                            .after(projection::project_ui_documents),
                        authored_credits_sequence_presentation::advance_authored_credits_sequence_presentations,
                        authored_rail_camera_presentation::hydrate_rail_cameras,
                        authored_rail_camera_presentation::update_rail_cameras
                            .after(authored_rail_camera_presentation::hydrate_rail_cameras),
                        authored_rail_camera_presentation::cleanup_rail_camera_scenes,
                        authored_button_attention_pulse_presentation::advance_authored_button_attention_pulse_presentations,
                    ),
                    (
                        authored_globe_scene_presentation::hydrate_globe_models,
                        authored_globe_scene_presentation::project_globe_prefabs,
                        authored_globe_marker_presentation::project_globe_markers,
                        authored_globe_marker_presentation::present_globe_marker_selection,
                        authored_globe_camera_framing::frame_globe_cameras,
                        authored_globe_interaction::target_selected_globe_marker
                            .after(authored_globe_marker_presentation::present_globe_marker_selection)
                            .after(authored_globe_marker_presentation::project_globe_markers)
                            .after(authored_globe_camera_framing::frame_globe_cameras),
                        authored_globe_interaction::turn_globe_to_selection
                            .after(authored_globe_interaction::target_selected_globe_marker),
                        authored_globe_interaction::rotate_globe
                            .after(authored_globe_interaction::turn_globe_to_selection),
                        authored_globe_scene_presentation::cleanup_globe_scenes,
                    ),
                )
                    .in_set(UiSet::Behavior),
            )
            .add_systems(
                Update,
                (
                    notification::request_notifications,
                    notification::acknowledge_notifications
                        .after(notification::request_notifications),
                    notification::reject_failed_notifications
                        .after(notification::request_notifications),
                )
                    .in_set(UiSet::Notifications),
            )
            .add_systems(
                Update,
                authored_ui_layout_participation::synchronize_authored_visibility_with_bevy_layout_participation
                    .after(UiSet::Notifications)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                PostUpdate,
                (
                    focused_or_selected_node_scroll_reveal::scroll_nearest_ancestor_to_reveal_focused_or_selected_node
                        .after(bevy::ui::UiSystems::Layout),
                    authored_ui_canvas_scaling_and_clipping::snap_authored_ui_canvas_descendant_edges_to_physical_pixels
                        .after(bevy::ui::UiSystems::PostLayout),
                    authored_ui_canvas_scaling_and_clipping::project_authored_scaled_canvas_clip_rectangles
                        .after(authored_ui_canvas_scaling_and_clipping::snap_authored_ui_canvas_descendant_edges_to_physical_pixels),
                    authored_globe_camera_framing::update_globe_camera_viewports
                        .after(bevy::ui::UiSystems::PostLayout),
                ),
            )
            .add_systems(
                Last,
                authored_text_edit_bevy_adaptation::suppress_fitting_authored_text_edit_scroll_during_render_extraction,
            );
        app.sub_app_mut(bevy::render::RenderApp).add_systems(
            bevy::render::ExtractSchedule,
            (
                authored_visual_layer_draw_order::place_authored_aspect_draws_in_owner_stack_slot,
                authored_text_physical_glyph_projection::preserve_authored_font_aspect_in_extracted_text_draws,
            ).chain()
                .after(bevy::ui_render::RenderUiSystems::ExtractCursor),
        );
    }
}

fn show_application_ui(mut commands: Commands, mut show: ResMut<UiRoleRequests>) {
    let owner = commands
        .spawn((ApplicationUiOwner, Visibility::Inherited))
        .id();
    show.request(UiDocumentRole::ApplicationRoot, owner);
}

fn show_in_game_hud(mut commands: Commands, mut show: ResMut<UiRoleRequests>) {
    let owner = commands.spawn((InGameUiOwner, Visibility::Inherited)).id();
    show.request(UiDocumentRole::InGameHud, owner);
    show.request(UiDocumentRole::InGamePersistentStatus, owner);
}

fn hide_in_game_hud(mut commands: Commands, owners: Query<Entity, With<InGameUiOwner>>) {
    for owner in &owners {
        commands.entity(owner).despawn();
    }
}
