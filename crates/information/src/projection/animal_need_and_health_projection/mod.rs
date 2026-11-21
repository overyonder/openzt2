use super::panel_binding_families::*;
use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_health::types::Escaped;
use openzt2_zoo_state::animal_health::types::Rampaging;
use openzt2_zoo_state::animal_health::types::Tranquilized;
use openzt2_zoo_state::animal_health::types::Treatment;
use openzt2_zoo_state::animal_health::types::Vitality;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_welfare::types::AnimalWelfare;
use openzt2_zoo_state::animal_welfare::types::EnvironmentNeed;
use openzt2_zoo_state::animal_welfare::types::ExerciseNeed;
use openzt2_zoo_state::animal_welfare::types::HealthNeed;
use openzt2_zoo_state::animal_welfare::types::Hunger;
use openzt2_zoo_state::animal_welfare::types::HygieneNeed;
use openzt2_zoo_state::animal_welfare::types::PrivacyNeed;
use openzt2_zoo_state::animal_welfare::types::RestNeed;
use openzt2_zoo_state::animal_welfare::types::SocialNeed;
use openzt2_zoo_state::animal_welfare::types::StimulationNeed;
use openzt2_zoo_state::animal_welfare::types::Thirst;
use openzt2_zoo_state::animal_welfare::types::WelfareBand;
use openzt2_zoo_state::animal_welfare::types::Q16_ONE;

use openzt2_interface_state::information::entity_selection_types::{InfoPanel, SelectedSubject};

pub(crate) fn project_live_animal_needs_to_visible_information_panels(
    panels: Query<
        (
            &ChildOf,
            Ref<InheritedVisibility>,
            Option<Ref<AnimalNeedFields>>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
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
    mut values: Query<(&UiValueBinding, &mut UiValue), AnimalNeedField>,
) {
    for (parent, visible, fields) in &panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !visible.get() {
            continue;
        }
        let Ok(needs) = animals.get(selection.0) else {
            continue;
        };
        if !selection.is_changed()
            && !visible.is_changed()
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
            && !fields.as_ref().is_some_and(Ref::is_changed)
        {
            continue;
        }
        for entity in fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((binding, mut value)) = values.get_mut(entity) else {
                continue;
            };
            let next = match &binding.0 {
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
            value.set_if_neq(UiValue(next));
        }
    }
}

pub(crate) fn project_live_animal_health_to_visible_information_panels(
    panels: Query<
        (
            &ChildOf,
            &InheritedVisibility,
            Option<&AnimalHealthValueFields>,
            Option<&AnimalHealthVisibilityFields>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
    animals: Query<
        (
            Option<&Vitality>,
            Option<Ref<Disease>>,
            Option<Ref<Treatment>>,
            Option<Ref<Tranquilized>>,
            Option<Ref<Escaped>>,
            Option<Ref<Rampaging>>,
            Option<Ref<Dead>>,
        ),
        With<Animal>,
    >,
    mut values: Query<(&UiValueBinding, &mut UiValue), AnimalHealthValueField>,
    mut visible_nodes: Query<(&UiVisibleBinding, &mut Visibility), AnimalHealthVisibilityField>,
) {
    for (parent, inherited, values_fields, visibility_fields) in &panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !inherited.get() {
            continue;
        }
        let Ok(health) = animals.get(selection.0) else {
            continue;
        };
        for entity in values_fields.into_iter().flat_map(RelationshipTarget::iter) {
            if let (Some(vitality), Ok((binding, mut value))) = (health.0, values.get_mut(entity)) {
                if matches!(
                    &binding.0,
                    UiIntegerPropertyBindingSource::AnimalVitalityPermille
                ) {
                    value.set_if_neq(UiValue((vitality.0.clamp(0.0, 1.0) * 1000.0).round() as i64));
                }
            }
        }
        for entity in visibility_fields
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((binding, mut visibility)) = visible_nodes.get_mut(entity) else {
                continue;
            };
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
