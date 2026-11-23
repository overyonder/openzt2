//! Object information, catalogue and Zoopedia screens.

pub mod catalogue;
mod information_entity_list_operations;
mod information_graph_operations;
mod information_list_types;
mod information_selection_action_operations;
mod information_selection_action_types;
mod information_ui_action_routing;
mod information_view_class_hydration;
mod information_view_filter_operations;
mod information_view_types;
pub mod overview;
mod projection;
mod selected_entity_ui_broadcasts;
mod zoopedia;

use bevy::prelude::*;
use information_view_types::InformationViewFilters;
use openzt2_interface_state::information::catalogue_types::PurchaseChoice;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::GameSet;

pub struct InformationPlugin;

impl Plugin for InformationPlugin {
    fn build(&self, app: &mut App) {
        projection::information_list_membership::install_management_list_memberships(app);
        app.add_observer(overview::overview_map_marker_operations::retire_overview_markers_when_subject_loses_inspectability);
        app.add_observer(projection::entity_editor_data_root_projection::unlink_retired_inspectable_world_subjects)
            .add_systems(
                Update,
                projection::entity_editor_data_root_projection::link_inspectable_world_subjects
                    .before(projection::entity_editor_data_root_projection::project_inspectable_entities_to_visible_entity_editor_data_roots)
                    .before(projection::information_list_projection::project_live_world_entities_to_visible_information_list_rows),
            )
            .init_resource::<InformationViewFilters>()
            .add_message::<PurchaseChoice>()
            .add_systems(
                Update,
                catalogue::animal_care_subject_projection::retain_selected_animal_for_care_catalogue
                    .in_set(GameSet::Ui)
                    .before(openzt2_interface_state::ui::UiSet::Projection),
            )
            .add_message::<openzt2_interface_state::information::overview::overview_types::ExportOverviewMap>()
            .add_systems(
                Update,
                (
                    catalogue::catalogue_type_list_row_count_operations::initialize_authored_catalogue_type_list_state,
                    catalogue::catalogue_filter_menu::populate_catalogue_filter_menu,
                    catalogue::catalogue_filter_menu::select_catalogue_filter_row
                        .after(catalogue::catalogue_type_list_row_count_operations::initialize_authored_catalogue_type_list_state)
                        .after(catalogue::catalogue_filter_menu::populate_catalogue_filter_menu),
                    catalogue::catalogue_type_list_activation::choose_catalogue_entry_from_activated_type_list_row,
                    catalogue::catalogue_type_list_activation::choose_catalogue_entry_from_activated_type_list
                        .after(catalogue::catalogue_filter_menu::select_catalogue_filter_row),
                    catalogue::adoption_list_operations::choose_species_from_activated_adoption_list_row
                        .after(catalogue::catalogue_type_list_activation::choose_catalogue_entry_from_activated_type_list_row)
                        .after(catalogue::catalogue_type_list_activation::choose_catalogue_entry_from_activated_type_list),
                    information_ui_action_routing::route_authored_information_actions_from_activated_ui_nodes
                        .after(catalogue::catalogue_type_list_row_count_operations::initialize_authored_catalogue_type_list_state)
                        .after(catalogue::catalogue_filter_menu::select_catalogue_filter_row)
                        .after(catalogue::catalogue_type_list_activation::choose_catalogue_entry_from_activated_type_list)
                        .before(openzt2_interface_state::shell::ShellNavigationAfterSettings),
                    (
                        zoopedia::zoopedia_navigation_activation::navigate_to_subject_from_pressed_zoopedia_table_of_contents_row
                            .after(information_ui_action_routing::route_authored_information_actions_from_activated_ui_nodes),
                        zoopedia::zoopedia_navigation_activation::navigate_to_subject_from_pressed_zoopedia_rich_content_link,
                    ).chain(),
                    catalogue::catalogue_selection_and_details_projection::select_catalogue_entry_and_open_purchase_information_document
                        .after(catalogue::adoption_list_operations::choose_species_from_activated_adoption_list_row)
                        .after(information_ui_action_routing::route_authored_information_actions_from_activated_ui_nodes),
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    (
                        information_view_class_hydration::hydrate_information_view_classes_from_canonical_world_definitions,
                        information_view_filter_operations::project_information_view_filters_to_world_entities,
                    )
                        .chain(),
                    (
                        information_view_filter_operations::hydrate_authored_world_view_filter_toggle_bindings
                            .run_if(resource_changed::<Assets<openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset>>.or_else(any_match_filter::<(
                                With<openzt2_interface_state::ui::authored_ui_selection_state::UiSelected>,
                                Or<(
                                    Changed<openzt2_interface_state::ui::authored_ui_action_projection_components::UiInformationActions>,
                                    Changed<openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner>,
                                    Added<openzt2_interface_state::ui::authored_ui_selection_state::UiSelected>,
                                )>,
                            )>)),
                        information_view_filter_operations::project_information_view_filters_to_authored_controls,
                    ).chain().after(openzt2_interface_state::ui::UiSet::Projection),
                    catalogue::animal_care_subject_projection::project_care_animal_name_and_icon,
                    (
                        catalogue::catalogue_type_list_row_count_operations::reconcile_authored_catalogue_type_lists,
                        catalogue::catalogue_type_list_row_projection::project_catalogue_entries_to_authored_type_list_rows
                            .in_set(openzt2_interface_state::information::catalogue_types::CatalogueTypeListRowProjection),
                    ).chain(),
                    (
                        catalogue::catalogue_research_overlay::attach_authored_catalogue_research_overlays,
                        catalogue::catalogue_research_overlay::hydrate_authored_catalogue_research_overlay_controls,
                        catalogue::catalogue_research_overlay::project_catalogue_research_overlays
                            .before(openzt2_interface_state::ui::UiSet::Interaction)
                            .after(openzt2_interface_state::information::catalogue_types::CatalogueTypeListRowProjection),
                    )
                        .chain(),
                    catalogue::catalogue_filter_menu::label_catalogue_filter_rows,
                    (
                    catalogue::adoption_list_operations::apply_authored_adoption_catalogue_tab_initialization,
                    (
                        catalogue::adoption_list_operations::select_first_available_species_after_adoption_catalogue_and_offer_inventory_are_both_available,
                        catalogue::adoption_list_operations::request_authored_adoption_list_row_count,
                        catalogue::adoption_list_operations::project_adoption_catalogue_entries_to_authored_rows
                            .after(catalogue::adoption_list_operations::select_first_available_species_after_adoption_catalogue_and_offer_inventory_are_both_available),
                    )
                        .run_if(catalogue::adoption_list_operations::an_authored_adoption_catalogue_is_visible),
                    catalogue::catalogue_selection_and_details_projection::dismiss_purchase_information_on_inspection_request,
                    catalogue::catalogue_selection_and_details_projection::project_purchase_information_document_visibility_from_selected_subject
                        .after(catalogue::catalogue_selection_and_details_projection::dismiss_purchase_information_on_inspection_request),
                    (
                        catalogue::catalogue_selection_and_details_projection::project_selected_catalogue_entry_to_purchase_information_document
                            .after(catalogue::catalogue_selection_and_details_projection::dismiss_purchase_information_on_inspection_request),
                        catalogue::catalogue_selection_and_details_projection::position_visible_purchase_information_rows_from_authored_dynamic_start,
                    ).chain(),
                    ),
                    (
                        catalogue::catalogue_preview_scene_projection::project_selected_catalogue_entry_into_preview_scene,
                        catalogue::catalogue_preview_scene_projection::synchronize_catalogue_preview_camera_activity_with_authored_image_visibility
                            .after(catalogue::catalogue_preview_scene_projection::project_selected_catalogue_entry_into_preview_scene),
                    ),
                    (
                        catalogue::catalogue_preview_scene_projection::hydrate_catalogue_preview_scene_prefab_render_trees
                            .after(catalogue::catalogue_preview_scene_projection::project_selected_catalogue_entry_into_preview_scene),
                        catalogue::catalogue_preview_scene_projection::attach_loaded_species_animation_to_catalogue_preview_renderables,
                        catalogue::catalogue_preview_scene_projection::apply_authored_catalogue_preview_initial_animation_repetition_policy_after_attachment,
                    ).chain(),
                    catalogue::catalogue_page_projection::project_catalogue_page,
                    projection::selected_entity_identity_projection::project_selected_entity_identity_to_authored_panel_members.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::selected_entity_identity_projection::project_selected_entity_name_to_authored_editable_fields.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::selected_entity_identity_projection::project_live_zoo_name_to_authored_editable_fields,
                    projection::selected_animal_need_list_projection::request_selected_animal_need_list_row_counts,
                    projection::selected_animal_need_list_projection::project_selected_animal_needs_to_authored_list_rows,
                    projection::animal_need_and_health_projection::project_live_animal_needs_to_visible_information_panels.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::animal_need_and_health_projection::project_live_animal_health_to_visible_information_panels.after(projection::panel_binding_families::bind_information_panel_fields),
                    (
                        (
                            selected_entity_ui_broadcasts::index_projected_selected_entity_ui_broadcast_targets,
                            selected_entity_ui_broadcasts::begin_selected_entity_ui_broadcasts,
                        ),
                        selected_entity_ui_broadcasts::dispatch_selected_entity_ui_broadcasts,
                    ).chain().before(openzt2_interface_state::ui::UiSet::Commands),
                )
                    .after(openzt2_interface_state::ui::UiSet::Projection)
                    .before(openzt2_interface_state::ui::UiSet::Behavior)
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    projection::fame_presentation::bind_authored_fame_fields,
                    projection::fame_presentation::project_fame_values_into_authored_fields,
                ).chain()
                    .after(openzt2_interface_state::ui::UiSet::Projection)
                    .before(openzt2_interface_state::ui::UiSet::Behavior)
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                projection::panel_binding_families::bind_information_panel_fields
                    .after(openzt2_interface_state::ui::UiSet::Projection)
                    .before(openzt2_interface_state::ui::UiSet::Behavior)
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    projection::selected_animal_summary_projection::project_selected_entity_information_section_visibility,
                    projection::selected_animal_summary_projection::project_selected_animal_identity_health_and_action_summary.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::animal_pickup_release_information_projection::project_selected_animal_pickup_release_and_conservation_controls
                        .after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::guest_status_projection::project_live_guest_status_to_visible_information_panels.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::staff_status_projection::project_maintenance_worker_duties_to_information_panel_controls.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::staff_status_projection::project_live_staff_status_to_visible_information_panels.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::zoo_status_projection::project_live_zoo_status_to_authored_information_bindings,
                    projection::animal_alert_button_projection::project_animal_alerts_to_find_animal_buttons,
                    (
                        projection::information_list_projection::project_live_world_entities_to_visible_information_list_rows,
                        projection::entity_editor_data_root_projection::project_inspectable_entities_to_visible_entity_editor_data_roots,
                    ).before(projection::information_row_field_binding::bind_authored_information_row_fields),
                    projection::information_row_field_binding::bind_authored_information_row_fields
                        .before(projection::information_list_row_identity_projection::project_world_subject_identity_to_information_list_rows)
                        .before(projection::information_list_row_value_projection::project_live_world_subject_values_to_information_list_rows),
                    projection::information_list_row_identity_projection::project_world_subject_identity_to_information_list_rows,
                    projection::information_list_row_value_projection::project_live_world_subject_values_to_information_list_rows,
                )
                    .after(openzt2_interface_state::ui::UiSet::Projection)
                    .before(openzt2_interface_state::ui::UiSet::Behavior)
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    zoopedia::zoopedia_page_hydration::hydrate_zoopedia_pages_from_authored_root_entry,
                    (
                        zoopedia::zoopedia_table_of_contents_projection::apply_authored_zoopedia_table_of_contents_expansion_changes
                            .after(zoopedia::zoopedia_table_of_contents_projection::project_zoopedia_table_of_contents_for_hydrated_pages),
                        zoopedia::zoopedia_table_of_contents_projection::reconcile_visible_zoopedia_entries,
                        zoopedia::zoopedia_table_of_contents_projection::project_zoopedia_table_of_contents_rows,
                    ).chain(),
                    zoopedia::zoopedia_table_of_contents_projection::project_zoopedia_table_of_contents_for_hydrated_pages
                        .after(zoopedia::zoopedia_page_hydration::hydrate_zoopedia_pages_from_authored_root_entry),
                    zoopedia::zoopedia_navigation_control_interaction_projection::project_zoopedia_history_availability_to_authored_navigation_controls
                        .after(zoopedia::zoopedia_page_hydration::hydrate_zoopedia_pages_from_authored_root_entry),
                    zoopedia::zoopedia_page_projection::project_localized_zoopedia_page_to_authored_information_document
                        .after(zoopedia::zoopedia_page_hydration::hydrate_zoopedia_pages_from_authored_root_entry),
                )
                    .after(openzt2_interface_state::ui::UiSet::Projection)
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    (
                        overview::canvas_hydration_operations::create_missing_authored_overview_map_canvas_entities,
                        overview::map_rasterization_operations::invalidate_changed_overview_map_sources,
                        (
                            overview::map_rasterization_operations::rasterize_dirty_overview_topology_canvases,
                            overview::terrain_rasterization_operations::rasterize_dirty_overview_terrain_and_water_canvases,
                        ),
                        overview::map_rasterization_operations::publish_pending_overview_map_images,
                    ).chain().after(openzt2_interface_state::ui::UiSet::Projection),
                    overview::overview_map_legend_operations::size_authored_overview_map_legends
                        .after(openzt2_interface_state::ui::UiSet::Projection),
                    (
                        overview::overview_map_legend_operations::bind_authored_overview_map_legend_controls,
                        overview::overview_map_legend_operations::toggle_overview_map_layers_from_pressed_legend_rows,
                        overview::overview_map_legend_operations::project_overview_legend_checkbox_selection,
                    ).chain().after(openzt2_interface_state::ui::UiSet::Projection),
                    (
                        overview::overview_map_marker_operations::hydrate_authored_overview_marker_layers
                            .run_if(
                                resource_changed::<Assets<openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset>>
                                    .or_else(any_match_filter::<Or<(
                                        Changed<openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiTablePolicy>,
                                        Changed<openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentMembers>,
                                    )>>),
                            ),
                        overview::overview_map_marker_operations::reconcile_changed_overview_subject_markers
                            .after(information_view_class_hydration::hydrate_information_view_classes_from_canonical_world_definitions),
                        overview::overview_map_marker_operations::project_world_subject_positions_to_overview_map_markers,
                    ).chain().after(openzt2_interface_state::ui::UiSet::Projection),
                    overview::overview_map_marker_operations::project_overhead_camera_position_to_overview_map_marker,
                    overview::overview_map_marker_operations::request_entity_selection_from_pressed_overview_map_markers,
                    projection::maintenance_status_projection::project_live_maintenance_status_to_visible_information_panels.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::facility_status_projection::project_selected_facility_status_to_visible_information_panels.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::habitat_and_tank_status_projection::project_selected_habitat_and_tank_status_to_visible_information_panels.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::show_status_projection::project_selected_show_schedule_status_to_visible_information_panels.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::transport_status_projection::project_selected_transport_status_to_visible_information_panels.after(projection::panel_binding_families::bind_information_panel_fields),
                    projection::scenario_status_projection::project_active_scenario_status_to_authored_information_bindings,
                    projection::progression_status_projection::project_progression_status_to_authored_information_bindings,
                )
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod tests;
