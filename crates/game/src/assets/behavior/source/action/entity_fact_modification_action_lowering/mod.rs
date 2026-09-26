//! Lowering of authored entity-fact modification actions into canonical modifications.

use std::io;

use openzt2_game_data::behavior::{
    action::{
        entity_role::BehaviorEntityRole,
        modification::{BehaviorFact, BehaviorFactModification, BehaviorFactModificationOperation},
    },
    action_record::BehaviorAction,
};

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_boolean_attribute_or_false, read_q16_attribute_or_default},
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

pub(super) fn lower_entity_fact_modification_action_source_node(
    modification_action_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if !matches!(
        modification_action_node.name.as_str(),
        "BFBehSetAttribute"
            | "BFAISubjectData"
            | "BFAITargetData"
            | "BFAIAttributeFloatMap"
            | "ZTBehChangeWaterDirtiness"
    ) {
        return Ok(None);
    }
    if modification_action_node.name == "ZTBehChangeWaterDirtiness" {
        return Ok(Some(BehaviorAction::FactModifications(vec![
            BehaviorFactModification {
                affected_entity_role: BehaviorEntityRole::Target,
                modified_fact: BehaviorFact::WaterDirtiness,
                modification_operation: if read_boolean_attribute_or_false(
                    modification_action_node,
                    "Clean",
                )? {
                    BehaviorFactModificationOperation::Clear
                } else {
                    BehaviorFactModificationOperation::Add
                },
                modification_value:
                    openzt2_game_data::behavior::scalar::BehaviorScalarQ16::FixedQ16(
                        read_q16_attribute_or_default(modification_action_node, "Delta", 0)?,
                    ),
            },
        ])));
    }
    let modification_owner_nodes: Vec<_> = if matches!(
        modification_action_node.name.as_str(),
        "BFAISubjectData" | "BFAITargetData" | "BFAIAttributeFloatMap"
    ) {
        vec![modification_action_node]
    } else {
        modification_action_node.element_children().collect()
    };
    let modifications = modification_owner_nodes
        .into_iter()
        .flat_map(|modification_owner_node| {
            let affected_entity_role = if matches!(
                modification_owner_node.name.as_str(),
                "BFAISubjectData" | "BFAIAttributeFloatMap"
            ) {
                BehaviorEntityRole::Subject
            } else {
                BehaviorEntityRole::Target
            };
            modification_owner_node
                .attributes
                .iter()
                .map(move |source_attribute| (affected_entity_role, source_attribute))
        })
        .filter(|(_, source_attribute)| source_attribute.name() != "b_SuppressFailureMessage")
        .map(|(affected_entity_role, source_attribute)| {
            Ok(BehaviorFactModification {
                affected_entity_role,
                modified_fact: lower_modifiable_behavior_fact(source_attribute.name())?,
                modification_operation: BehaviorFactModificationOperation::Add,
                modification_value: super::super::scalar::lower_behavior_scalar_q16(
                    source_attribute.value(),
                )?,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(Some(BehaviorAction::FactModifications(modifications)))
}

fn lower_modifiable_behavior_fact(authored_fact_name: &str) -> io::Result<BehaviorFact> {
    Ok(match authored_fact_name {
        "amusement" => BehaviorFact::Amusement,
        "bathroom" => BehaviorFact::Bathroom,
        "breath" => BehaviorFact::Breath,
        "exercise" | "excercise" => BehaviorFact::Exercise,
        "happiness" => BehaviorFact::Happiness,
        "health" => BehaviorFact::Health,
        "hunger" => BehaviorFact::Hunger,
        "dessert" => BehaviorFact::Dessert,
        "gift" => BehaviorFact::Gift,
        "viewanimals" => BehaviorFact::ViewAnimals,
        "f_AteFavoriteFood" => BehaviorFact::AteFavoriteFood,
        "f_AteNonFavoriteFood" => BehaviorFact::AteNonFavoriteFood,
        "hygiene" => BehaviorFact::Hygiene,
        "privacy" => BehaviorFact::Privacy,
        "reproduction" => BehaviorFact::Reproduction,
        "rest" => BehaviorFact::Rest,
        "social" => BehaviorFact::Social,
        "space" => BehaviorFact::Space,
        "stimulation" => BehaviorFact::Stimulation,
        "thirst" => BehaviorFact::Thirst,
        "waterDirtiness" => BehaviorFact::WaterDirtiness,
        "f_FoodLevel" => BehaviorFact::FoodLevel,
        "f_BoneLevel" => BehaviorFact::BoneLevel,
        "f_TrashLevel" => BehaviorFact::TrashLevel,
        "f_FilterDirtyLevel" => BehaviorFact::FilterDirtiness,
        "f_ConstructionLevel" => BehaviorFact::Construction,
        "f_Damage" => BehaviorFact::Damage,
        "b_Opened" => BehaviorFact::Opened,
        "b_Moving" => BehaviorFact::Moving,
        "b_Disease" => BehaviorFact::Disease,
        "b_Pregnant" => BehaviorFact::Pregnancy,
        "b_Rampage" => BehaviorFact::Rampage,
        "b_Escaped" => BehaviorFact::Escaped,
        "lifespan" => BehaviorFact::Lifespan,
        "isSwimming" => BehaviorFact::Swimming,
        "inWater" => BehaviorFact::InWater,
        "onLand" => BehaviorFact::OnLand,
        "f_PaintLevel" => BehaviorFact::PaintLevel,
        "f_PaintLevel_1" => BehaviorFact::PaintLevel1,
        "f_PaintLevel_2" => BehaviorFact::PaintLevel2,
        "f_PaintLevel_3" => BehaviorFact::PaintLevel3,
        "f_PaintLevel_4" => BehaviorFact::PaintLevel4,
        "f_IceLevel" => BehaviorFact::IceLevel,
        "f_departurePoints" | "f_DeparturePoints" => BehaviorFact::DeparturePoints,
        "b_Talking" => BehaviorFact::Talking,
        "b_Dead" => BehaviorFact::Dead,
        "b_Frozen" => BehaviorFact::Frozen,
        "b_InGlacier" => BehaviorFact::InGlacier,
        "b_NurseYoung" => BehaviorFact::NursingYoung,
        _ => {
            return Err(invalid_behavior_source_data(format!(
                "unmapped modifiable behavior fact {authored_fact_name}"
            )));
        }
    })
}
