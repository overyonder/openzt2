use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use crate::plugins::{
    aquatic::aquatic_simulation_types::{TankCapacity, TankGeometry, WaterQuality},
    habitat::habitat_types::{HabitatRegion, HabitatSummary},
    ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiValue,
        authored_ui_node_projection_components::UiValueBinding,
        authored_ui_node_projection_components::UiVisibleBinding,
    },
};

use super::super::entity_selection_types::InfoPanel;

pub(in crate::plugins::information) fn project_selected_habitat_and_tank_status_to_visible_information_panels(
    information_panels: Query<(Entity, &InfoPanel, &InheritedVisibility)>,
    habitat_and_tank_subjects: Query<(
        Option<&HabitatRegion>,
        Option<&HabitatSummary>,
        Option<&TankGeometry>,
        Option<&WaterQuality>,
        Option<&TankCapacity>,
    )>,
    mut authored_integer_values: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
    mut authored_boolean_visibility: Query<(&UiDocumentOwner, &UiVisibleBinding, &mut Visibility)>,
) {
    for (panel_entity, information_panel, inherited_visibility) in &information_panels {
        if !inherited_visibility.get() {
            continue;
        }
        let Ok((habitat_region, habitat_summary, tank_geometry, water_quality, tank_capacity)) =
            habitat_and_tank_subjects.get(information_panel.subject)
        else {
            continue;
        };

        for (document_owner, property_binding, mut projected_value) in &mut authored_integer_values
        {
            if document_owner.0 != panel_entity {
                continue;
            }
            let Some(next_value) = (match &property_binding.0 {
                UiIntegerPropertyBindingSource::HabitatAreaMilliSquareMetres => {
                    habitat_region.map(|region| {
                        convert_finite_f32_to_nearest_milli_integer(region.area_square_metres)
                    })
                }
                UiIntegerPropertyBindingSource::HabitatLandMilliSquareMetres => habitat_summary
                    .map(|summary| {
                        convert_finite_f32_to_nearest_milli_integer(summary.land_area_square_metres)
                    }),
                UiIntegerPropertyBindingSource::HabitatWaterMilliSquareMetres => habitat_summary
                    .map(|summary| {
                        convert_finite_f32_to_nearest_milli_integer(
                            summary.water_area_square_metres,
                        )
                    }),
                UiIntegerPropertyBindingSource::HabitatBiomeAreaMilliSquareMetres { row } => {
                    habitat_summary
                        .and_then(|summary| {
                            summary.biome_areas_square_metres.get(usize::from(*row))
                        })
                        .map(|(_, area)| convert_finite_f32_to_nearest_milli_integer(*area))
                }
                UiIntegerPropertyBindingSource::TankWaterQualityPermille => {
                    water_quality.map(|quality| i64::from(quality.0))
                }
                UiIntegerPropertyBindingSource::TankUsedLitres => tank_capacity
                    .map(|capacity| convert_finite_f32_to_nearest_milli_integer(capacity.used)),
                UiIntegerPropertyBindingSource::TankRequiredLitres => tank_capacity
                    .map(|capacity| convert_finite_f32_to_nearest_milli_integer(capacity.required)),
                _ => None,
            }) else {
                continue;
            };
            projected_value.0 = next_value;
        }

        for (document_owner, property_binding, mut projected_visibility) in
            &mut authored_boolean_visibility
        {
            if document_owner.0 != panel_entity {
                continue;
            }
            let should_be_visible = match &property_binding.0 {
                UiBooleanPropertyBindingSource::HabitatBreached => {
                    habitat_summary.is_some_and(|summary| summary.boundary_is_breached)
                }
                UiBooleanPropertyBindingSource::IsTank => tank_geometry.is_some(),
                _ => continue,
            };
            *projected_visibility = if should_be_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn convert_finite_f32_to_nearest_milli_integer(value: f32) -> i64 {
    if value.is_finite() {
        (value * 1000.0).round() as i64
    } else {
        0
    }
}
