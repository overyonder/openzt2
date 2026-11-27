use openzt2_interface_state::ui::UiSet;
mod authored_click_catcher_lifetime;
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
mod authored_window_interaction;
mod authored_zoo_message_lists;
mod controller_ui_action_routing;
pub mod cursor;
mod drop_list;
mod focused_or_selected_node_scroll_reveal;
mod graph_generated_geometry;
mod graph_history_series_rendering;
mod graph_presentation;
mod graph_presentation_types;
mod graph_tick_projection;
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
mod authored_scrollbar;

use bevy::{
    ecs::schedule::common_conditions::on_message,
    picking::events::{PointerDrag, PointerPress},
    prelude::*,
};
use openzt2_interface_state::input::input_types::ActionRequest;

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
        projection::register_document_attachment_cleanup(app);
        app.add_observer(notification::discard_notification_without_projected_alert);
        app.add_observer(authored_globe_interaction::cancel_turn_when_globe_marker_is_lost);
        app.add_observer(
            authored_timed_action_sequence::release_selection_when_timed_sequence_disappears,
        );
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
        use openzt2_interface_state::information::entity_selection_types::{
            SelectionChanged, SelectionRequest,
        };
        app.add_observer(
            focused_or_selected_node_scroll_reveal::reveal_in_nearest_authored_scroll_area,
        )
        .add_message::<authored_rail_camera_presentation::RailDeveloperCommand>()
        .add_message::<authored_rail_camera_presentation::RailWaypointsEdited>()
        .add_message::<SelectionRequest>()
        .add_message::<SelectionChanged>()
        .add_observer(world_subject_selection::clear_world_ui_selection_on_loss_of_inspectability)
        .add_systems(
            Update,
            world_subject_selection::apply_world_ui_selection
                .after(GameSet::Intent)
                .before(GameSet::Ui),
        );
        app.register_required_components::<openzt2_world_state::world_spawn::world_membership_types::WorldRoot, openzt2_interface_state::information::zoo_message_types::ZooMessages>()
            .register_required_components::<InGameUiOwner, authored_zoo_message_lists::LatestZooMessagePresentation>()
            .add_message::<ShowUiDocument>()
            .add_message::<SetUiListRowCount>()
            .add_message::<ShowUiRole>()
            .add_message::<ui_document_lifecycle_contracts::ShowUiRoleInScope>()
            .add_message::<openzt2_interface_state::information::award_presentation_types::AwardProgramEffect>()
            .add_message::<ui_document_lifecycle_contracts::AuthoredUiDocumentOperation>()
            .add_message::<HideUiDocument>()
            .init_resource::<active_authored_ui_context::AuthoredFocusScope>()
            .add_observer(authored_ui_focus_navigation_and_activation::authored_focus_entered)
            .add_observer(authored_ui_focus_navigation_and_activation::authored_focus_left)
            .add_observer(authored_ui_change_activation_dispatch::activate_initial_deselection)
            .add_observer(authored_ui_change_activation_dispatch::activate_initial_visibility)
            .add_message::<authored_ui_node_projection_components::UiSliderValueChanged>()
            .add_observer(slider::apply_native_slider_value)
            .add_message::<authored_ui_activation_contracts::UiNodeActivated>()
            .add_message::<authored_ui_activation_contracts::ActivateAuthoredUiTool>()
            .add_message::<authored_ui_presentation_action_application::RequestUiPresentationAction>()
            .add_message::<animation::UiAnimationPlaybackRequest>()
            .add_message::<authored_timed_action_sequence::TimedUiPresentationCommitted>()
            .add_message::<authored_timed_action_sequence::UiSelectionWaitEnded>()
            .add_message::<notification_contracts::ShowNotification>()
            .add_message::<notification_contracts::NotificationPresented>()
            .add_message::<notification_contracts::NotificationRejected>()
            .add_message::<ui_document_asset_load_failure::UiDocumentAssetLoadFailed>()
            .add_observer(authored_text_edit_bevy_adaptation::dispatch_authored_change_activation_after_bevy_text_edit)
            .add_systems(
                Update,
                (
                    notification::load_authored_guest_thought_fragments,
                    notification::project_selected_guest_thoughts
                        .after(notification::load_authored_guest_thought_fragments),
                    notification::initialize_authored_guest_thought_text
                        .after(notification::project_selected_guest_thoughts),
                ).in_set(UiSet::DomainProjection),
            )
            .add_systems(
                Update,
                projection::ui_document_opening::open_authored_award_panel
                    .in_set(UiSet::Routing),
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
                    (
                        projection::ui_document_opening::ingest_ui_document_opening_requests,
                        projection::ui_document_opening::project_ui_documents,
                        world_subject_selection::show_information_for_world_ui_selection,
                    ).chain(),
                    graph_presentation::clip_authored_graph_generated_geometry
                        .after(projection::ui_document_opening::project_ui_documents),
                    authored_zoo_message_lists::update_latest_zoo_message_presentation,
                    main_toolbar::restore_main_toolbar_toggle_selection
                        .after(projection::ui_document_opening::project_ui_documents),
                    authored_text_edit_bevy_adaptation::initialize_authored_text_edits_as_bevy_editable_text
                        .after(projection::ui_document_opening::project_ui_documents),
                    (
                        projection::authored_list_row_projection::record_requested_ui_list_row_counts
                            .run_if(on_message::<SetUiListRowCount>),
                        authored_zoo_message_lists::size_zoo_message_lists
                            .after(authored_zoo_message_lists::update_latest_zoo_message_presentation),
                        projection::authored_list_row_projection::reconcile_changed_ui_list_row_counts
                            .after(projection::authored_list_row_projection::record_requested_ui_list_row_counts)
                            .after(authored_zoo_message_lists::size_zoo_message_lists),
                        projection::authored_scenario_goal_projection::project_authored_scenario_goal_trees,
                        projection::authored_list_row_projection::project_authored_award_group_rows,
                        projection::authored_list_row_projection::hydrate_authored_award_group_rows
                            .after(projection::authored_list_row_projection::project_authored_award_group_rows),
                        projection::authored_list_row_projection::project_authored_award_milestone_presentation
                            .after(projection::authored_list_row_projection::hydrate_authored_award_group_rows),
                        projection::authored_list_row_projection::materialize_ready_ui_list_rows
                            .after(projection::authored_list_row_projection::reconcile_changed_ui_list_row_counts),
                        authored_zoo_message_lists::bind_zoo_message_rows
                            .after(projection::authored_list_row_projection::materialize_ready_ui_list_rows),
                    ).after(projection::ui_document_opening::project_ui_documents),
                    (
                        slider::link_authored_slider_thumbs,
                        authored_ui_canvas_scaling_and_clipping::link_ui_canvas_members,
                        authored_ui_canvas_scaling_and_clipping::project_authored_ui_canvas_viewport_metrics,
                        authored_tree_expansion_and_row_indentation::bind_tree_items_to_containing_rows,
                    ).after(projection::authored_list_row_projection::materialize_ready_ui_list_rows),
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
                        active_authored_ui_context::resolve_authored_focus_scope
                            .after(authored_pointer_events::activate_authored_pointer_events)
                            .after(authored_text_edit_bevy_adaptation::discard_pending_edits_and_focus_from_disabled_text_edits),
                        active_authored_ui_context::project_native_widget_interaction_from_authored_enablement
                            .before(authored_text_edit_bevy_adaptation::discard_pending_edits_and_focus_from_disabled_text_edits),
                        authored_ui_focus_navigation_and_activation::update_active_authored_focus
                            .after(active_authored_ui_context::resolve_authored_focus_scope)
                            .before(slider::adjust_focused_sliders)
                            .before(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions),
                        authored_ui_focus_navigation_and_activation::release_edit_focus_when_pressing_elsewhere
                            .before(authored_pointer_events::activate_authored_pointer_events),
                        authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions
                            .run_if(on_message::<ActionRequest>),
                        authored_globe_interaction::interact_with_globe.before(authored_pointer_events::activate_authored_pointer_events),
                        authored_pointer_events::activate_authored_pointer_events,
                        authored_composite_button_activation_forwarding::forward_authored_composite_button_activations_to_projected_child_controls.after(authored_pointer_events::activate_authored_pointer_events),
                        drop_list::toggle_or_close_authored_drop_list_displays_from_activations
                            .after(authored_composite_button_activation_forwarding::forward_authored_composite_button_activations_to_projected_child_controls),
                        (
                            authored_click_catcher_lifetime::dismiss_pressed_click_catchers
                                .run_if(on_message::<authored_ui_activation_contracts::UiNodeActivated>),
                            authored_click_catcher_lifetime::synchronize_click_catcher_timers,
                            authored_click_catcher_lifetime::expire_active_click_catchers,
                        ).chain().after(authored_pointer_events::activate_authored_pointer_events),
                        slider::apply_slider_bounds,
                        slider::apply_authored_slider_pointer_events
                            .after(active_authored_ui_context::resolve_authored_focus_scope)
                            .run_if(on_message::<PointerPress>.or_eager(on_message::<PointerDrag>)),
                        slider::adjust_focused_sliders
                            .run_if(on_message::<ActionRequest>)
                            .after(authored_ui_focus_navigation_and_activation::update_active_authored_focus)
                            .before(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions),
                        authored_text_edit_bevy_adaptation::discard_pending_edits_and_focus_from_disabled_text_edits,
                    ),
                    (
                        controller_ui_action_routing::route_controller_authored_actions,
                        single_receiver_ui_cancellation::dismiss_one_active_ui_activity_from_cancel_requests
                            .after(active_authored_ui_context::resolve_authored_focus_scope)
                            .run_if(on_message::<ActionRequest>)
                            .after(authored_text_edit_bevy_adaptation::discard_pending_edits_and_focus_from_disabled_text_edits),
                        authored_ui_focus_navigation_and_activation::activate_focused_authored_ui_node_from_confirm
                            .run_if(on_message::<ActionRequest>)
                            .after(authored_ui_focus_navigation_and_activation::navigate_authored_ui_focus_from_actions)
                            .after(authored_pointer_events::activate_authored_pointer_events),
                        authored_toggle_selection_transitions::transition_pressed_authored_toggles
                            .after(authored_composite_button_activation_forwarding::forward_authored_composite_button_activations_to_projected_child_controls),
                        authored_ui_interaction_visual_presentation::present_authored_ui_interaction_visual_state
                            .after(active_authored_ui_context::project_native_widget_interaction_from_authored_enablement)
                            .after(authored_pointer_events::activate_authored_pointer_events)
                            .after(authored_toggle_selection_transitions::transition_pressed_authored_toggles),
                        authored_tree_expansion_and_row_indentation::toggle_authored_tree_expansion_from_presses.after(authored_pointer_events::activate_authored_pointer_events),
                        authored_tree_expansion_and_row_indentation::update_tree_row_indentation,
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
                (
                    cursor::project_cursor_window_scale,
                    cursor::resolve_requested_window_cursor.after(cursor::project_cursor_window_scale),
                    cursor::apply_cached_or_unscaled_window_cursor.after(cursor::resolve_requested_window_cursor),
                    cursor::synthesize_requested_cursor_image.after(cursor::apply_cached_or_unscaled_window_cursor)
                        .run_if(on_message::<cursor::SynthesizeCursorImage>),
                ).in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    animation::tick_authored_ui_animations,
                    authored_text_edit_bevy_adaptation::update_authored_text_edit_watchers,
                    authored_ui_presentation_action_application::presentation_batch::apply_ordered_presentation_batches
                        .after(animation::tick_authored_ui_animations),
                    projection::ui_document_opening::apply_authored_ui_document_opening_operations
                        .after(authored_ui_presentation_action_application::presentation_batch::apply_ordered_presentation_batches),
                    authored_timed_action_sequence::dispatch_due_authored_sequence_steps
                        .after(authored_ui_presentation_action_application::presentation_batch::apply_ordered_presentation_batches),
                    authored_timed_action_sequence::acknowledge_settled_scenario_ui_selections
                        .after(authored_timed_action_sequence::dispatch_due_authored_sequence_steps),
                )
                    .in_set(UiSet::Commands),
            )
            .add_systems(
                PostUpdate,
                (
                    authored_fame_star_presentation::isolate_fame_star_particles,
                    authored_fame_star_presentation::set_fame_star_camera_visibility
                        .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
                )
                    .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
            )
            .add_systems(
                Update,
                (
                    (
                        graph_presentation::update_visible_authored_graph_drawings,
                        graph_tick_projection::write_graph_tick_text
                            .after(graph_presentation::update_visible_authored_graph_drawings),
                        held_button_repeat::repeat_held_buttons,
                        authored_window_interaction::move_authored_windows_from_pointer_drags,
                        authored_drag_gesture::apply_authored_drag_gestures_to_bevy_layout_or_scroll_position,
                        (
                            authored_tooltip_presentation::resolve_tooltip_hover_targets,
                            (
                                authored_tooltip_presentation::tick_active_tooltip_timers,
                                authored_tooltip_presentation::project_tooltip_text_from_hover_binding,
                                authored_tooltip_presentation::position_floating_tooltips,
                            ),
                        ).chain(),
                        authored_multi_icon_presentation::present_multi_icons
                            .after(projection::ui_document_opening::project_ui_documents),
                        (
                            authored_fame_star_presentation::create_fame_star_scenes,
                            authored_fame_star_presentation::present_fame_stars
                                .after(authored_fame_star_presentation::create_fame_star_scenes),
                            authored_fame_star_presentation::reveal_delayed_fame_stars
                                .after(authored_fame_star_presentation::present_fame_stars),
                            authored_fame_star_presentation::expire_fame_star_transients,
                        ),
                        authored_rail_camera_presentation::hydrate_rail_cameras,
                        authored_rail_camera_presentation::route_rail_developer_hotkeys
                            .after(authored_rail_camera_presentation::hydrate_rail_cameras),
                        authored_rail_camera_presentation::apply_rail_developer_edits
                            .after(authored_rail_camera_presentation::route_rail_developer_hotkeys),
                        authored_rail_camera_presentation::rebuild_edited_rail_clips
                            .after(authored_rail_camera_presentation::apply_rail_developer_edits)
                            .run_if(on_message::<authored_rail_camera_presentation::RailWaypointsEdited>),
                        authored_rail_camera_presentation::synchronize_rail_animation_playback
                            .after(authored_rail_camera_presentation::apply_rail_developer_edits),
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
                        authored_globe_interaction::link_selected_globe_marker_anchors
                            .after(authored_globe_marker_presentation::project_globe_markers),
                        authored_globe_interaction::retarget_selected_globe_turns
                            .after(authored_globe_interaction::link_selected_globe_marker_anchors)
                            .after(authored_globe_camera_framing::frame_globe_cameras),
                        authored_globe_interaction::advance_globe_selection_turns
                            .after(authored_globe_interaction::retarget_selected_globe_turns),
                        authored_globe_interaction::rotate_globe
                            .after(authored_globe_interaction::advance_globe_selection_turns),
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
                    notification::populate_projected_notification_alerts,
                    notification::report_effectively_visible_notifications
                        .after(notification::populate_projected_notification_alerts),
                    notification::reject_failed_notifications
                        .after(notification::request_notifications),
                )
                    .in_set(UiSet::Notifications),
            )
            .add_systems(
                Update,
                authored_ui_layout_participation::project_authored_layout_participation
                    .after(UiSet::Notifications)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                Update,
                animation::apply_authored_animation_playback_requests
                    .after(UiSet::Notifications)
                    .run_if(on_message::<animation::UiAnimationPlaybackRequest>)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                PostUpdate,
                (
                    (
                        focused_or_selected_node_scroll_reveal::scroll_nearest_ancestor_to_reveal_focused_or_selected_node,
                        slider::project_slider_thumb_layout,
                        slider::project_authored_values_to_native_sliders,
                        slider::project_authored_slider_value_labels
                            .after(slider::bind_authored_slider_value_labels),
                        slider::bind_authored_slider_value_labels,
                        authored_scrollbar::project_scrollbar_availability,
                        authored_ui_canvas_scaling_and_clipping::apply_authored_canvas_layout_corrections,
                    ).before(bevy::ui::UiSystems::Layout),
                    (
                        authored_ui_canvas_scaling_and_clipping::project_ui_canvas_pixel_transform,
                        authored_ui_canvas_scaling_and_clipping::snap_authored_ui_canvas_descendant_edges_to_physical_pixels,
                    ).chain().after(bevy::ui::UiSystems::Layout).before(bevy::ui::UiSystems::Clipping),
                    authored_globe_model_bounds::update_instantiated_globe_model_bounds
                        .after(bevy::transform::TransformSystems::Propagate)
                        .after(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
                    authored_rail_camera_presentation::project_rail_lighting
                        .after(bevy::app::AnimationSystems)
                        .before(bevy::transform::TransformSystems::Propagate),
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
        if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render.add_systems(
                bevy::render::ExtractSchedule,
                (
                    authored_visual_layer_draw_order::place_authored_aspect_draws_in_owner_stack_slot,
                    authored_text_physical_glyph_projection::preserve_authored_font_aspect_in_extracted_text_draws,
                ).after(bevy::ui_render::RenderUiSystems::ExtractInlineImages),
            );
        }
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
