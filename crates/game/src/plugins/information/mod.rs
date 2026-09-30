//! Object information, catalogue and Zoopedia screens.

pub(crate) mod catalogue;
pub(crate) mod catalogue_types;
pub(crate) mod entity_selection_types;
mod information_entity_list_operations;
mod information_graph_operations;
pub(crate) mod information_graph_types;
mod information_list_types;
mod information_selection_action_operations;
mod information_selection_action_types;
mod information_ui_action_routing;
mod information_view_class_hydration;
mod information_view_filter_operations;
mod information_view_types;
pub(crate) mod overview;
mod projection;
mod selected_entity_ui_broadcasts;
mod selection;
mod zoopedia;

use bevy::prelude::*;
use catalogue_types::{PurchaseChoice, SelectedCareAnimal, SelectedCatalogueEntry};
use entity_selection_types::{SelectedEntity, SelectionChanged, SelectionRequest};
use information_view_types::InformationViewFilters;
use selection::{
    apply_selection_requests, bind_info_panel_subject, clear_removed_selection,
    reconcile_selected_entity_information,
};

use crate::{application_lifecycle::GamePhase, application_schedule::GameSet};

pub(super) struct InformationPlugin;

impl Plugin for InformationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedEntity>()
            .init_resource::<SelectedCatalogueEntry>()
            .init_resource::<SelectedCareAnimal>()
            .init_resource::<InformationViewFilters>()
            .add_message::<SelectionRequest>()
            .add_message::<SelectionChanged>()
            .add_message::<PurchaseChoice>()
            .add_systems(OnExit(GamePhase::InGame), catalogue::animal_care_subject_projection::clear_care_animal)
            .add_message::<overview::overview_types::ExportOverviewMap>()
            .add_systems(
                Update,
                (
                    catalogue::catalogue_type_list_row_count_operations::populate_activated_authored_catalogue_type_list_row_counts,
                    catalogue::catalogue_filter_menu::populate_catalogue_filter_menu,
                    catalogue::catalogue_filter_menu::select_catalogue_filter_row,
                    catalogue::catalogue_type_list_activation::choose_catalogue_entry_from_activated_type_list_row,
                    catalogue::catalogue_type_list_activation::choose_catalogue_entry_from_activated_type_list,
                    catalogue::adoption_list_operations::choose_species_from_activated_adoption_list_row,
                    information_ui_action_routing::route_authored_information_actions_from_activated_ui_nodes
                        .before(crate::plugins::shell::ShellNavigationAfterSettings),
                    zoopedia::zoopedia_navigation_activation::navigate_to_subject_from_pressed_zoopedia_table_of_contents_row,
                    zoopedia::zoopedia_navigation_activation::navigate_to_subject_from_pressed_zoopedia_rich_content_link,
                    apply_selection_requests,
                    catalogue::catalogue_selection_and_details_projection::select_catalogue_entry_and_open_purchase_information_document,
                    catalogue::animal_care_subject_projection::retain_selected_animal_for_care_catalogue,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    (
                        information_view_class_hydration::hydrate_information_view_classes_from_canonical_world_definitions,
                        information_view_filter_operations::project_information_view_filters_to_world_entities_and_authored_controls,
                        clear_removed_selection,
                        bind_info_panel_subject,
                        catalogue::animal_care_subject_projection::project_care_animal_name_and_icon,
                    )
                        .chain(),
                    catalogue::catalogue_type_list_row_count_operations::request_visible_catalogue_type_list_row_counts,
                    catalogue::catalogue_type_list_row_projection::project_catalogue_entries_to_authored_type_list_rows,
                    catalogue::catalogue_filter_menu::label_catalogue_filter_rows,
                    (
                        catalogue::adoption_list_operations::apply_authored_adoption_catalogue_tab_initialization,
                        (
                            catalogue::adoption_list_operations::select_first_available_species_after_adoption_catalogue_and_offer_inventory_are_both_available,
                            catalogue::adoption_list_operations::request_authored_adoption_list_row_count,
                            catalogue::adoption_list_operations::project_adoption_catalogue_entries_to_authored_rows,
                        )
                            .chain()
                            .run_if(catalogue::adoption_list_operations::an_authored_adoption_catalogue_is_visible),
                    )
                        .chain(),
                    (
                        catalogue::catalogue_selection_and_details_projection::dismiss_purchase_information_on_inspection_tool_request,
                        catalogue::catalogue_selection_and_details_projection::project_selected_catalogue_entry_to_purchase_information_document,
                        catalogue::catalogue_selection_and_details_projection::position_visible_purchase_information_rows_from_authored_dynamic_start,
                    )
                        .chain(),
                    (
                        catalogue::catalogue_preview_scene_projection::project_selected_catalogue_entry_into_preview_scene,
                        catalogue::catalogue_preview_scene_projection::synchronize_catalogue_preview_camera_activity_with_authored_image_visibility
                            .after(catalogue::catalogue_preview_scene_projection::project_selected_catalogue_entry_into_preview_scene),
                    ),
                    catalogue::catalogue_preview_scene_projection::hydrate_catalogue_preview_scene_prefab_render_trees,
                    catalogue::catalogue_preview_scene_projection::attach_loaded_species_animation_to_catalogue_preview_renderables,
                    catalogue::catalogue_preview_scene_projection::apply_authored_catalogue_preview_initial_animation_repetition_policy_after_attachment,
                    catalogue::catalogue_preview_scene_projection::retire_catalogue_preview_scenes_without_ui_owners,
                    catalogue::catalogue_page_projection::project_catalogue_page,
                    (
                        projection::selected_entity_identity_projection::project_authored_definition_name_to_selected_entity_panel,
                        projection::selected_entity_identity_projection::project_live_name_to_selected_entity_panel,
                        projection::selected_entity_identity_projection::project_selected_entity_name_to_authored_editable_fields,
                        projection::selected_entity_identity_projection::project_live_zoo_name_to_authored_editable_fields,
                    )
                        .chain(),
                    projection::selected_animal_need_list_projection::request_selected_animal_need_list_row_counts,
                    projection::selected_animal_need_list_projection::project_selected_animal_needs_to_authored_list_rows,
                    projection::animal_need_and_health_projection::project_live_animal_needs_to_visible_information_panels,
                    projection::animal_need_and_health_projection::project_live_animal_health_to_visible_information_panels,
                    reconcile_selected_entity_information,
                    selected_entity_ui_broadcasts::dispatch_selected_entity_ui_broadcasts
                        .before(crate::plugins::ui::UiSet::Behavior),
                )
                    .chain()
                    .after(crate::plugins::ui::UiSet::Projection)
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    projection::selected_animal_summary_projection::project_selected_entity_information_section_visibility,
                    projection::selected_animal_summary_projection::project_selected_animal_identity_health_and_action_summary,
                    projection::animal_pickup_release_information_projection::project_selected_animal_pickup_release_and_conservation_controls,
                    projection::guest_status_projection::project_live_guest_status_to_visible_information_panels,
                    projection::staff_status_projection::project_maintenance_worker_duties_to_information_panel_controls,
                    projection::staff_status_projection::project_live_staff_status_to_visible_information_panels,
                    projection::zoo_status_projection::project_live_zoo_status_to_authored_information_bindings,
                    projection::information_list_projection::project_live_world_entities_to_visible_information_list_rows,
                    projection::information_list_row_identity_projection::project_world_subject_identity_to_information_list_rows,
                    projection::information_list_row_value_projection::project_live_world_subject_values_to_information_list_rows,
                    projection::entity_editor_data_root_projection::project_inspectable_entities_to_visible_entity_editor_data_roots,
                )
                    .chain()
                    .after(projection::animal_need_and_health_projection::project_live_animal_health_to_visible_information_panels)
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    zoopedia::zoopedia_page_hydration::hydrate_zoopedia_pages_from_authored_root_entry,
                    zoopedia::zoopedia_table_of_contents_projection::project_zoopedia_table_of_contents_for_hydrated_pages,
                    zoopedia::zoopedia_table_of_contents_projection::apply_authored_zoopedia_table_of_contents_expansion_changes,
                    zoopedia::zoopedia_table_of_contents_projection::request_zoopedia_table_of_contents_row_counts,
                    zoopedia::zoopedia_table_of_contents_projection::project_zoopedia_table_of_contents_rows,
                    zoopedia::zoopedia_navigation_operations::apply_pending_zoopedia_entries_to_hydrated_pages,
                    zoopedia::zoopedia_navigation_control_interaction_projection::project_zoopedia_history_availability_to_authored_navigation_controls,
                    zoopedia::zoopedia_page_projection::project_localized_zoopedia_page_to_authored_information_document,
                )
                    .chain()
                    .after(projection::zoo_status_projection::project_live_zoo_status_to_authored_information_bindings)
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    overview::canvas_hydration_operations::create_missing_authored_overview_map_canvas_entities,
                    overview::overview_map_legend_operations::request_authored_overview_map_legend_row_entities,
                    overview::overview_map_legend_operations::bind_authored_overview_map_layer_data_to_legend_rows,
                    overview::map_rasterization_operations::rebuild_changed_overview_map_canvas_images_from_world_state,
                    overview::overview_map_legend_operations::toggle_overview_map_layers_from_pressed_legend_rows,
                    overview::overview_map_marker_operations::create_missing_authored_overview_map_marker_entities,
                    overview::overview_map_marker_operations::project_world_subject_positions_to_overview_map_markers,
                    overview::overview_map_marker_operations::project_overhead_camera_position_to_overview_map_marker,
                    overview::overview_map_marker_operations::request_entity_selection_from_pressed_overview_map_markers,
                    overview::overview_map_marker_operations::remove_overview_map_markers_for_retired_subjects,
                    projection::maintenance_status_projection::project_live_maintenance_status_to_visible_information_panels,
                    projection::facility_status_projection::project_selected_facility_status_to_visible_information_panels,
                    projection::habitat_and_tank_status_projection::project_selected_habitat_and_tank_status_to_visible_information_panels,
                    projection::show_status_projection::project_selected_show_schedule_status_to_visible_information_panels,
                    projection::transport_status_projection::project_selected_transport_status_to_visible_information_panels,
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
