use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use crate::plugins::{
    animal_health::types::{Dead, Disease, Escaped, Rampaging, Tranquilized, Treatment, Vitality},
    animal_lifecycle::types::Animal,
    animal_welfare::types::{
        AnimalWelfare, EnvironmentNeed, ExerciseNeed, HealthNeed, Hunger, HygieneNeed, PrivacyNeed,
        RestNeed, SocialNeed, StimulationNeed, Thirst, WelfareBand, Q16_ONE,
    },
    ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiValue,
        authored_ui_node_projection_components::UiValueBinding,
        authored_ui_node_projection_components::UiVisibleBinding,
    },
};

use super::super::entity_selection_types::InfoPanel;

pub(in crate::plugins::information) fn project_live_animal_needs_to_visible_information_panels(
    panels: Query<(Entity, Ref<InfoPanel>, &InheritedVisibility)>,
    animals: Query<
        (
            Ref<Hunger>,
            Ref<Thirst>,
            Ref<RestNeed>,
            Ref<PrivacyNeed>,
            Ref<SocialNeed>,
            Ref<ExerciseNeed>,
            Ref<StimulationNeed>,
            Ref<EnvironmentNeed>,
            Ref<HygieneNeed>,
            Ref<HealthNeed>,
            Ref<AnimalWelfare>,
            Ref<WelfareBand>,
        ),
        With<Animal>,
    >,
    mut values: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
) {
    for (panel_entity, panel, visible) in &panels {
        if !visible.get() {
            continue;
        }
        let Ok(needs) = animals.get(panel.subject) else {
            continue;
        };
        if !panel.is_changed()
            && !needs.0.is_changed()
            && !needs.1.is_changed()
            && !needs.2.is_changed()
            && !needs.3.is_changed()
            && !needs.4.is_changed()
            && !needs.5.is_changed()
            && !needs.6.is_changed()
            && !needs.7.is_changed()
            && !needs.8.is_changed()
            && !needs.9.is_changed()
            && !needs.10.is_changed()
        {
            continue;
        }
        for (owner, binding, mut value) in &mut values {
            if owner.0 != panel_entity {
                continue;
            }
            value.0 = match &binding.0 {
                UiIntegerPropertyBindingSource::AnimalHungerQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.0 .0)
                }
                UiIntegerPropertyBindingSource::AnimalThirstQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.1 .0)
                }
                UiIntegerPropertyBindingSource::AnimalRestQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.2 .0)
                }
                UiIntegerPropertyBindingSource::AnimalPrivacyQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.3 .0)
                }
                UiIntegerPropertyBindingSource::AnimalSocialQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.4 .0)
                }
                UiIntegerPropertyBindingSource::AnimalExerciseQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.5 .0)
                }
                UiIntegerPropertyBindingSource::AnimalStimulationQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.6 .0)
                }
                UiIntegerPropertyBindingSource::AnimalEnvironmentQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.7 .0)
                }
                UiIntegerPropertyBindingSource::AnimalHygieneQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.8 .0)
                }
                UiIntegerPropertyBindingSource::AnimalHealthNeedQ16Permille => {
                    convert_q16_animal_need_value_to_permille(needs.9 .0)
                }
                UiIntegerPropertyBindingSource::AnimalWelfarePermille => i64::from(needs.10 .0),
                _ => continue,
            };
        }
    }
}

pub(in crate::plugins::information) fn project_live_animal_health_to_visible_information_panels(
    panels: Query<(Entity, Ref<InfoPanel>, &InheritedVisibility)>,
    animals: Query<
        (
            Ref<Vitality>,
            Option<Ref<Disease>>,
            Option<Ref<Treatment>>,
            Option<Ref<Tranquilized>>,
            Option<Ref<Escaped>>,
            Option<Ref<Rampaging>>,
            Option<Ref<Dead>>,
        ),
        With<Animal>,
    >,
    mut values: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
    mut visible_nodes: Query<(&UiDocumentOwner, &UiVisibleBinding, &mut Visibility)>,
) {
    for (panel_entity, panel, inherited) in &panels {
        if !inherited.get() {
            continue;
        }
        let Ok(health) = animals.get(panel.subject) else {
            continue;
        };
        for (owner, binding, mut value) in &mut values {
            if owner.0 == panel_entity
                && matches!(
                    &binding.0,
                    UiIntegerPropertyBindingSource::AnimalVitalityPermille
                )
            {
                let next = (health.0 .0.clamp(0.0, 1.0) * 1000.0).round() as i64;
                if value.0 != next {
                    value.0 = next;
                }
            }
        }
        for (owner, binding, mut visibility) in &mut visible_nodes {
            if owner.0 != panel_entity {
                continue;
            }
            let present = match &binding.0 {
                UiBooleanPropertyBindingSource::AnimalDiseased => health.1.is_some(),
                UiBooleanPropertyBindingSource::AnimalUnderTreatment => health.2.is_some(),
                UiBooleanPropertyBindingSource::AnimalTranquilized => health.3.is_some(),
                UiBooleanPropertyBindingSource::AnimalEscaped => health.4.is_some(),
                UiBooleanPropertyBindingSource::AnimalRampaging => health.5.is_some(),
                UiBooleanPropertyBindingSource::AnimalDead => health.6.is_some(),
                _ => continue,
            };
            let next = if present {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != next {
                *visibility = next;
            }
        }
    }
}

fn convert_q16_animal_need_value_to_permille(animal_need_q16_value: i32) -> i64 {
    i64::from(animal_need_q16_value.clamp(0, 1000 * Q16_ONE) / Q16_ONE)
}
