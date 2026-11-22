use openzt2_interface_state::ui::UiSet;
mod authored_fame_star_presentation;
mod authored_globe_camera_framing;
mod authored_globe_interaction;
mod authored_globe_marker_presentation;
mod authored_globe_model_bounds;
mod authored_globe_rotation_geometry;
use openzt2_game_data::ui_document::document::UiDocumentRole;
pub mod active_authored_ui_context;
pub mod animation;
pub mod authored_pointer_events;
use openzt2_interface_state::ui::authored_button_runtime_policy;
mod authored_composite_button_activation_forwarding;
mod authored_drag_gesture;
pub mod authored_globe_presentation_types;
mod authored_globe_scene_presentation;
mod authored_grid_layout_projection;
pub mod authored_hotkey_keyboard_activation;
pub mod authored_image_selection_diagnostic_override;
pub mod authored_modal_presentation;
pub mod authored_multi_icon_presentation;
mod authored_rail_camera_presentation;
#[cfg(debug_assertions)]
mod modal_render_diagnostics;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types;
pub mod authored_text_edit_bevy_adaptation;
pub mod authored_text_physical_glyph_projection;
mod authored_timed_action_sequence;
pub mod authored_toggle_selection_transitions;
mod authored_tool_and_edit_widget;
pub mod authored_tooltip_presentation;
pub mod authored_tree_expansion_and_row_indentation;
use openzt2_interface_state::ui::authored_ui_action_projection_components;
use openzt2_interface_state::ui::authored_ui_activation_contracts;
mod authored_ui_canvas_scaling_and_clipping;
pub mod authored_ui_change_activation_dispatch;
mod authored_ui_focus_navigation_and_activation;
use openzt2_interface_state::ui::authored_ui_focus_state;
pub mod authored_ui_image_content_binding;
use openzt2_interface_state::ui::authored_ui_integer_range_components;
pub mod authored_ui_interaction_enabled_binding;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state;
mod authored_ui_interaction_visual_presentation;
pub mod authored_ui_layout_participation;
use openzt2_interface_state::ui::authored_ui_node_projection_components;
pub mod authored_ui_presentation_action_application;
pub mod authored_ui_presentation_slot_bindings;
pub mod authored_ui_selection_binding;
mod authored_ui_target_lookup;
use openzt2_interface_state::ui::authored_ui_selection_state;
mod authored_ui_source_rectangle_presentation;
pub mod authored_ui_text_content_binding;
pub mod authored_ui_text_layout_presentation;
use openzt2_interface_state::ui::authored_ui_visual_types;
pub mod authored_visual_layer_draw_order;
pub mod authored_wheel_scroll;
pub mod authored_wheel_scroll_diagnostic_types;
mod authored_window_interaction;
mod authored_zoo_message_lists;
mod controller_ui_action_routing;
pub mod cursor;
mod drop_list;
mod focused_or_selected_node_scroll_reveal;
mod full_canvas_click_catcher;
mod graph_generated_geometry;
mod graph_history_series_rendering;
mod graph_presentation;
mod graph_presentation_types;
mod held_button_repeat;
pub mod localized_ui_text_writing;
mod main_toolbar;
mod notification;
use openzt2_interface_state::ui::notification_contracts;
pub mod picking;
pub mod projection;
pub mod save_dialog_cancellation;
pub mod scenario_objective_status_visual_classification;
mod single_receiver_ui_cancellation;
pub mod slider;
mod world_subject_selection;
use openzt2_interface_state::ui::ui_document_asset_load_failure;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts;

use self::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use self::cursor::SetWaitCursor;
mod authored_scrollbar;

use bevy::prelude::*;

use self::ui_document_lifecycle_contracts::{
    HideUiDocument, InGameUiOwner, ShowUiDocument, ShowUiRole,
};
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::GameSet;

pub struct GameUiPlugin;

#[derive(Component)]
struct ApplicationUiOwner;

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        cursor::initialize(app);
        picking::initialize(app);
        app.add_systems(PostUpdate,
            authored_ui_source_rectangle_presentation::project_changed_authored_image_source_rectangles
                .before(bevy::ui::UiSystems::Layout));
        app.configure_sets(
            PostUpdate,
            (
                // Let animation apply its input commands before the UI tree walk.
                bevy::ui::UiSystems::Stack.after_ignore_deferred(bevy::app::AnimationSystems),
                // Keep the UI clip commands out of the automatic visibility flush.
                bevy::ui::UiSystems::Clipping.after_ignore_deferred(
                    bevy::camera::visibility::VisibilitySystems::VisibilityPropagate,
                ),
            ),
        );
        authored_ui_presentation_action_application::register_presentation_observers(app);
        use openzt2_interface_state::information::entity_selection_types::{
            SelectionChanged, SelectionRequest,
        };
        app.add_observer(
            focused_or_selected_node_scroll_reveal::reveal_in_nearest_authored_scroll_area,
        )
        .add_message::<authored_rail_camera_presentation::RailDeveloperCommand>()
        .add_message::<SelectionRequest>()
        .add_message::<SelectionChanged>()
        .add_observer(world_subject_selection::clear_world_ui_selection_on_loss_of_inspectability)
        .add_systems(
            Update,
            world_subject_selection::apply_world_ui_selection
                .after(GameSet::Intent)
                .before(GameSet::Ui),
        );
        app.add_message::<ShowUiDocument>()
            .add_message::<SetWaitCursor>()
            .add_message::<SetUiListRowCount>()
            .add_message::<ShowUiRole>()
            .add_message::<ui_document_lifecycle_contracts::AuthoredUiDocumentOperation>()
            .add_message::<HideUiDocument>()
            .add_observer(authored_ui_focus_navigation_and_activation::authored_focus_entered)
            .add_observer(authored_ui_focus_navigation_and_activation::authored_focus_left)
            .add_observer(authored_ui_change_activation_dispatch::activate_initial_deselection)
            .add_observer(authored_ui_change_activation_dispatch::activate_initial_visibility)
            .add_message::<authored_ui_node_projection_components::UiSliderValueChanged>()
            .add_observer(slider::apply_native_slider_value)
            .add_message::<authored_ui_activation_contracts::UiNodeActivated>()
            .add_message::<authored_ui_activation_contracts::ActivateAuthoredUiTool>()
            .add_message::<authored_ui_presentation_action_application::RequestUiPresentationAction>()
            .add_message::<animation::UiAnimationCompleted>()
            .add_message::<notification_contracts::ShowNotification>()
            .add_message::<notification_contracts::NotificationPresented>()
            .add_message::<notification_contracts::NotificationRejected>()
            .add_message::<ui_document_asset_load_failure::UiDocumentAssetLoadFailed>()
            .add_observer(authored_text_edit_bevy_adaptation::dispatch_authored_change_activation_after_bevy_text_edit)
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
                    (
                        projection::ui_document_opening::ingest_ui_document_opening_requests,
                        projection::ui_document_opening::project_ui_documents,
                        projection::ui_document_opening::apply_ready_ui_document_opening_effects,
                        world_subject_selection::show_information_for_world_ui_selection,
                    ).chain(),
                    graph_presentation::clip_authored_graph_generated_geometry
                        .after(projection::ui_document_opening::project_ui_documents),
                    main_toolbar::initialize_main_toolbar.after(projection::ui_document_opening::project_ui_documents),
                    authored_text_edit_bevy_adaptation::initialize_authored_text_edits_as_bevy_editable_text
                        .after(projection::ui_document_opening::project_ui_documents),
                    (
                        projection::authored_list_row_projection::record_requested_ui_list_row_counts,
                        authored_zoo_message_lists::size_zoo_message_lists,
                        projection::authored_list_row_projection::reconcile_changed_ui_list_row_counts,
                        projection::authored_list_row_projection::materialize_ready_ui_list_rows,
                        authored_zoo_message_lists::bind_zoo_message_rows,
                    ).chain().after(projection::ui_document_opening::project_ui_documents),
                    (
                        slider::link_authored_slider_thumbs,
                        authored_ui_canvas_scaling_and_clipping::link_ui_canvas_members,
                        authored_ui_canvas_scaling_and_clipping::project_authored_ui_canvas_viewport_metrics,
                        authored_ui_canvas_scaling_and_clipping::apply_authored_canvas_layout_corrections,
                    ).chain().after(projection::authored_list_row_projection::materialize_ready_ui_list_rows),
                )
                    .in_set(UiSet::Projection),
            )
            .add_systems(
                Update,
                main_toolbar::reflect_accepted_camera_mode_in_main_toolbar.in_set(UiSet::DomainProjection),
            )
            .add_systems(
                Update,
                (
                    (
                        authored_hotkey_keyboard_activation::activate_projected_authored_hotkeys_from_keyboard_input,
                        authored_ui_focus_navigation_and_activation::update_active_authored_focus
                            .after(authored_pointer_events::activate_authored_pointer_events)
                            .before(slider::adjust_focused_sliders)
                            .before(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions),
                        authored_ui_focus_navigation_and_activation::release_edit_focus_when_pressing_elsewhere
                            .before(authored_pointer_events::activate_authored_pointer_events),
                        authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions,
                        authored_globe_interaction::interact_with_globe.before(authored_pointer_events::activate_authored_pointer_events),
                        authored_pointer_events::activate_authored_pointer_events,
                        authored_composite_button_activation_forwarding::forward_authored_composite_button_activations_to_projected_child_controls.after(authored_pointer_events::activate_authored_pointer_events),
                        drop_list::toggle_or_close_authored_drop_list_displays_from_activations
                            .after(authored_composite_button_activation_forwarding::forward_authored_composite_button_activations_to_projected_child_controls),
                        full_canvas_click_catcher::hide_activated_or_elapsed_full_canvas_click_catchers.after(authored_pointer_events::activate_authored_pointer_events),
                        slider::apply_slider_bounds,
                        slider::apply_authored_slider_pointer_events
                            .after(authored_pointer_events::activate_authored_pointer_events),
                        slider::adjust_focused_sliders
                            .after(authored_ui_focus_navigation_and_activation::update_active_authored_focus)
                            .before(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions),
                        slider::project_native_slider_values
                            .after(slider::apply_slider_bounds)
                            .after(slider::adjust_focused_sliders),
                        authored_text_edit_bevy_adaptation::discard_pending_edits_and_focus_from_disabled_text_edits,
                    ),
                    (
                        controller_ui_action_routing::route_controller_authored_actions,
                        single_receiver_ui_cancellation::dismiss_one_active_ui_activity_from_cancel_requests
                            .after(authored_text_edit_bevy_adaptation::discard_pending_edits_and_focus_from_disabled_text_edits),
                        authored_ui_focus_navigation_and_activation::activate_focused_authored_ui_node_from_confirm
                            .after(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions)
                            .after(authored_pointer_events::activate_authored_pointer_events),
                        authored_toggle_selection_transitions::transition_pressed_authored_toggles
                            .after(authored_composite_button_activation_forwarding::forward_authored_composite_button_activations_to_projected_child_controls),
                        authored_ui_interaction_visual_presentation::present_authored_ui_interaction_visual_state
                            .after(authored_pointer_events::activate_authored_pointer_events)
                            .after(authored_toggle_selection_transitions::transition_pressed_authored_toggles),
                        authored_tree_expansion_and_row_indentation::toggle_authored_tree_expansion_from_presses.after(authored_pointer_events::activate_authored_pointer_events),
                        authored_tree_expansion_and_row_indentation::present_authored_tree_children_from_expansion
                            .after(authored_tree_expansion_and_row_indentation::toggle_authored_tree_expansion_from_presses),
                        authored_tree_expansion_and_row_indentation::project_authored_tree_depth_into_containing_row_indentation,
                        authored_ui_change_activation_dispatch::dispatch_authored_value_change_activations
                            .after(slider::apply_slider_bounds)
                            .after(slider::adjust_focused_sliders)
                            .after(slider::apply_authored_slider_pointer_events),

                    ),
                )
                    .in_set(UiSet::Interaction),
            )
            .add_systems(
                Update,
                authored_tool_and_edit_widget::route_authored_tool_and_edit_activations.in_set(UiSet::Routing),
            )
            .add_systems(
                Update,
                (cursor::apply_wait_cursor_requests, cursor::present_cursor)
                    .chain()
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    animation::advance_ui_animations,
                    animation::dispatch_ui_animation_commands
                        .after(animation::advance_ui_animations),
                    authored_ui_presentation_action_application::route_presentation_actions,
                    projection::ui_document_opening::apply_authored_ui_document_opening_operations
                        .after(authored_ui_presentation_action_application::route_presentation_actions),
                    authored_timed_action_sequence::advance_authored_timed_action_sequences_and_activate_due_proxies
                        .after(authored_ui_presentation_action_application::route_presentation_actions),
                )
                    .in_set(UiSet::Commands),
            )
            .add_systems(
                Update,
                (
                    (
                        graph_presentation::update_visible_authored_graph_drawings,
                        held_button_repeat::repeat_held_buttons,
                        authored_wheel_scroll::scroll_captured_authored_list_or_window_from_mouse_wheel,
                        authored_window_interaction::move_authored_windows_from_pointer_drags,
                        authored_drag_gesture::apply_authored_drag_gestures_to_bevy_layout_or_scroll_position,
                        authored_tooltip_presentation::update_authored_tooltip_presentations,
                        authored_multi_icon_presentation::present_multi_icons
                            .after(projection::ui_document_opening::project_ui_documents),
                        (authored_fame_star_presentation::create_fame_star_scenes,
                         authored_fame_star_presentation::hydrate_fame_stars,
                         authored_fame_star_presentation::frame_fame_star_models,
                         authored_fame_star_presentation::present_fame_stars).chain(),
                        authored_rail_camera_presentation::hydrate_rail_cameras,
                        authored_rail_camera_presentation::route_rail_developer_hotkeys
                            .after(authored_rail_camera_presentation::hydrate_rail_cameras),
                        authored_rail_camera_presentation::update_rail_cameras
                            .after(authored_rail_camera_presentation::route_rail_developer_hotkeys),
                        authored_rail_camera_presentation::project_rail_lighting
                            .after(authored_rail_camera_presentation::update_rail_cameras),
                    ),
                    (
                        authored_globe_scene_presentation::hydrate_globe_models,
                        authored_globe_scene_presentation::project_globe_prefabs,
                        authored_globe_marker_presentation::project_globe_markers
                            .after(authored_globe_camera_framing::cache_globe_spheres),
                        authored_globe_marker_presentation::present_globe_marker_selection,
                        authored_globe_camera_framing::cache_globe_spheres,
                        authored_globe_camera_framing::frame_globe_cameras
                            .after(authored_globe_camera_framing::cache_globe_spheres),
                        authored_globe_interaction::target_selected_globe_marker
                            .after(authored_globe_marker_presentation::present_globe_marker_selection)
                            .after(authored_globe_marker_presentation::project_globe_markers)
                            .after(authored_globe_camera_framing::frame_globe_cameras),
                        authored_globe_interaction::turn_globe_to_selection
                            .after(authored_globe_interaction::target_selected_globe_marker),
                        authored_globe_interaction::rotate_globe
                            .after(authored_globe_interaction::turn_globe_to_selection),
                        authored_globe_interaction::highlight_hovered_globe_marker
                            .after(authored_globe_interaction::rotate_globe),
                        authored_globe_interaction::play_globe_spin_sound
                            .after(authored_globe_interaction::rotate_globe),
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
                    (
                        focused_or_selected_node_scroll_reveal::scroll_nearest_ancestor_to_reveal_focused_or_selected_node,
                        slider::project_slider_thumb_layout,
                        slider::present_slider_value_labels,
                        authored_scrollbar::project_scrollbar_availability,
                    ).chain().before(bevy::ui::UiSystems::Layout),
                    (
                        authored_ui_canvas_scaling_and_clipping::project_ui_canvas_pixel_transform,
                        authored_ui_canvas_scaling_and_clipping::snap_authored_ui_canvas_descendant_edges_to_physical_pixels,
                    ).chain().after(bevy::ui::UiSystems::Layout).before(bevy::ui::UiSystems::Clipping),
                    authored_globe_model_bounds::update_instantiated_globe_model_bounds
                        .after(bevy::transform::TransformSystems::Propagate)
                        .after(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
                    authored_globe_camera_framing::update_globe_camera_viewports
                        .after(bevy::ui::UiSystems::PostLayout),
                    // Measured in the same frame as Bevy's text layout, so a
                    // newly revealed label is never drawn with a stale offset.
                    authored_ui_text_layout_presentation::align_authored_single_line_text_from_measured_glyph_run
                        .after(bevy::ui::UiSystems::PostLayout),
                ),
            );
        #[cfg(debug_assertions)]
        app.add_systems(
            PostUpdate,
            modal_render_diagnostics::trace_changed_modal_render_state
                .after(bevy::ui::UiSystems::PostLayout),
        );
        app.sub_app_mut(bevy::render::RenderApp).add_systems(
            bevy::render::ExtractSchedule,
            (
                authored_visual_layer_draw_order::place_authored_aspect_draws_in_owner_stack_slot,
                authored_text_physical_glyph_projection::preserve_authored_font_aspect_in_extracted_text_draws,
            ).chain()
                .after(bevy::ui_render::RenderUiSystems::ExtractInlineImages),
        );
    }
}

fn show_application_ui(mut commands: Commands, mut show: MessageWriter<ShowUiRole>) {
    let owner = commands
        .spawn((ApplicationUiOwner, Visibility::Inherited))
        .id();
    show.write(ShowUiRole {
        role: UiDocumentRole::ApplicationRoot,
        owner,
    });
}

fn show_in_game_hud(mut commands: Commands, mut show: MessageWriter<ShowUiRole>) {
    let owner = commands.spawn((InGameUiOwner, Visibility::Inherited)).id();
    show.write(ShowUiRole {
        role: UiDocumentRole::InGameHud,
        owner,
    });
    show.write(ShowUiRole {
        role: UiDocumentRole::InGamePersistentStatus,
        owner,
    });
}

fn hide_in_game_hud(mut commands: Commands, owners: Query<Entity, With<InGameUiOwner>>) {
    for owner in &owners {
        commands.entity(owner).despawn();
    }
}
