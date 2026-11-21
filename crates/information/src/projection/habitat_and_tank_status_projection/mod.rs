use super::panel_binding_families::*;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankCapacity;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterQuality;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;

use openzt2_interface_state::information::entity_selection_types::{InfoPanel, SelectedSubject};

pub(crate) fn project_selected_habitat_and_tank_status_to_visible_information_panels(
    information_panels: Query<
        (
            &ChildOf,
            Ref<InheritedVisibility>,
            Option<Ref<HabitatFields>>,
            Option<Ref<HabitatVisibilityFields>>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
    habitat_and_tank_subjects: Query<(
        Option<&HabitatRegion>,
        Option<&HabitatSummary>,
        Option<&TankGeometry>,
        Option<&WaterQuality>,
        Option<&TankCapacity>,
    )>,
    mut authored_integer_values: Query<(&UiValueBinding, &mut UiValue), HabitatField>,
    mut authored_boolean_visibility: Query<
        (&UiVisibleBinding, &mut Visibility),
        HabitatVisibilityField,
    >,
) {
    for (parent, inherited_visibility, fields, visible_fields) in &information_panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !inherited_visibility.get() {
            continue;
        }
        let Ok((habitat_region, habitat_summary, tank_geometry, water_quality, tank_capacity)) =
            habitat_and_tank_subjects.get(selection.0)
        else {
            continue;
        };

        for entity in fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((property_binding, mut projected_value)) =
                authored_integer_values.get_mut(entity)
            else {
                continue;
            };
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

        for entity in visible_fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((property_binding, mut projected_visibility)) =
                authored_boolean_visibility.get_mut(entity)
            else {
                continue;
            };
            let should_be_visible = match &property_binding.0 {
                UiBooleanPropertyBindingSource::HabitatBreached => {
                    habitat_summary.is_some_and(|summary| summary.boundary_is_breached)
                }
                UiBooleanPropertyBindingSource::IsTank => tank_geometry.is_some(),
                _ => continue,
            };
            projected_visibility.set_if_neq(if should_be_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
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
