//! Conversion of authored behavior candidate qualifiers into eligibility facts.

use std::io;

use openzt2_game_data::behavior::eligibility::{
    BehaviorCandidateEligibilityFact, BehaviorCandidateEligibilityRequirements,
    BehaviorCandidateRole, BehaviorConditionEligibilityFact, BehaviorEligibilityFactComparison,
    BehaviorEligibilityFactInput, BehaviorEligibilityFactJunction, BehaviorEligibilityFactValue,
    BehaviorEntityStateEligibilityFact, BehaviorRelationshipEligibilityFact,
    BehaviorSpatialEligibilityFact, BehaviorTaxonomyEligibilityFact,
    BehaviorWelfareEligibilityFact,
};
use openzt2_game_data::AssetId;

use super::{find_direct_child_behavior_source_node, invalid_behavior_source_data};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

pub(super) fn lower_behavior_task_candidate_eligibility_requirements(
    behavior_task_node: &DataNode,
) -> io::Result<Vec<BehaviorCandidateEligibilityRequirements>> {
    let Some(task_creation_node) =
        find_direct_child_behavior_source_node(behavior_task_node, "BFAICreateData")
    else {
        return Ok(Vec::new());
    };
    task_creation_node
        .element_children()
        .filter_map(|candidate_group_node| {
            let candidate_role = if candidate_group_node.name.starts_with("Subjects") {
                BehaviorCandidateRole::Subject
            } else if candidate_group_node.name.starts_with("Targets") {
                BehaviorCandidateRole::Target
            } else if candidate_group_node.name.starts_with("Objects") {
                BehaviorCandidateRole::Object
            } else {
                return None;
            };
            Some((candidate_group_node, candidate_role))
        })
        .map(|(candidate_group_node, candidate_role)| {
            let candidate_type_identifiers = candidate_group_node
                .element_children()
                .filter(|child_node| !child_node.name.starts_with("Qualifiers"))
                .map(|child_node| AssetId::from_key(&child_node.name.trim().to_ascii_lowercase()))
                .collect();
            let qualifier_nodes = candidate_group_node
                .element_children()
                .filter(|child_node| child_node.name.starts_with("Qualifiers"))
                .collect::<Vec<_>>();
            let eligibility_junction = if qualifier_nodes
                .iter()
                .any(|qualifier_node| qualifier_node.name.contains("_AND"))
            {
                BehaviorEligibilityFactJunction::All
            } else {
                BehaviorEligibilityFactJunction::Any
            };
            let eligibility_facts = qualifier_nodes
                .into_iter()
                .flat_map(|qualifier_node| qualifier_node.attributes.iter())
                .map(|qualifier_attribute| {
                    lower_behavior_eligibility_fact(
                        qualifier_attribute.name(),
                        qualifier_attribute.value(),
                    )
                })
                .collect::<io::Result<Vec<_>>>()?;
            Ok(BehaviorCandidateEligibilityRequirements {
                candidate_role,
                eligibility_fact_junction: eligibility_junction,
                candidate_type_identifiers,
                candidate_type_junction: if candidate_group_node.name.ends_with("_AND") {
                    BehaviorEligibilityFactJunction::All
                } else {
                    BehaviorEligibilityFactJunction::Any
                },
                eligibility_facts,
            })
        })
        .filter(
            |eligibility_group_result: &io::Result<BehaviorCandidateEligibilityRequirements>| {
                eligibility_group_result
                    .as_ref()
                    .map_or(true, |eligibility_group| {
                        !eligibility_group.candidate_type_identifiers.is_empty()
                            || !eligibility_group.eligibility_facts.is_empty()
                    })
            },
        )
        .collect()
}

fn lower_behavior_eligibility_fact(
    authored_fact_name: &str,
    authored_fact_value: &str,
) -> io::Result<BehaviorCandidateEligibilityFact> {
    use BehaviorEligibilityFactInput::{
        Condition, Relationship, Spatial, State, Taxonomy, Welfare,
    };

    let eligibility_input = match authored_fact_name {
        "b_Adult" => State(BehaviorEntityStateEligibilityFact::Adult),
        "b_Attacked" => State(BehaviorEntityStateEligibilityFact::Attacked),
        "b_Dead" => State(BehaviorEntityStateEligibilityFact::Dead),
        "b_Disease" => State(BehaviorEntityStateEligibilityFact::Diseased),
        "b_Electric" => State(BehaviorEntityStateEligibilityFact::Electric),
        "b_InCloningCenter" => State(BehaviorEntityStateEligibilityFact::InCloningCenter),
        "b_Male" => State(BehaviorEntityStateEligibilityFact::Male),
        "b_Moving" => State(BehaviorEntityStateEligibilityFact::Moving),
        "b_NurseYoung" => State(BehaviorEntityStateEligibilityFact::NurseYoung),
        "b_Old" => State(BehaviorEntityStateEligibilityFact::Old),
        "b_Pregnant" => State(BehaviorEntityStateEligibilityFact::Pregnant),
        "b_Rampage" => State(BehaviorEntityStateEligibilityFact::Rampaging),
        "b_ScentEmitter" => State(BehaviorEntityStateEligibilityFact::ScentEmitter),
        "b_Teleport" => State(BehaviorEntityStateEligibilityFact::Teleportable),
        "b_Talking" => State(BehaviorEntityStateEligibilityFact::Talking),
        "b_Frozen" => State(BehaviorEntityStateEligibilityFact::Frozen),
        "canMate" => State(BehaviorEntityStateEligibilityFact::CanMate),
        "hasMate" => State(BehaviorEntityStateEligibilityFact::HasMate),
        "inHabitat" => State(BehaviorEntityStateEligibilityFact::InHabitat),
        "isInShow" => State(BehaviorEntityStateEligibilityFact::InShow),
        "isSubjectInTA" => State(BehaviorEntityStateEligibilityFact::InTrainingArea),
        "isSwimming" => State(BehaviorEntityStateEligibilityFact::Swimming),
        "isFloating" => State(BehaviorEntityStateEligibilityFact::Floating),
        "isElevated" => State(BehaviorEntityStateEligibilityFact::Elevated),
        "amusement" => Welfare(BehaviorWelfareEligibilityFact::Amusement),
        "bathroom" => Welfare(BehaviorWelfareEligibilityFact::Bathroom),
        "bloom" => Welfare(BehaviorWelfareEligibilityFact::Bloom),
        "breath" => Welfare(BehaviorWelfareEligibilityFact::Breath),
        "dessert" => Welfare(BehaviorWelfareEligibilityFact::Dessert),
        "exercise" | "excercise" => Welfare(BehaviorWelfareEligibilityFact::Exercise),
        "happiness" => Welfare(BehaviorWelfareEligibilityFact::Happiness),
        "health" => Welfare(BehaviorWelfareEligibilityFact::Health),
        "hunger" => Welfare(BehaviorWelfareEligibilityFact::Hunger),
        "hygiene" => Welfare(BehaviorWelfareEligibilityFact::Hygiene),
        "incubation" => Welfare(BehaviorWelfareEligibilityFact::Incubation),
        "lifespan" => Welfare(BehaviorWelfareEligibilityFact::Lifespan),
        "privacy" => Welfare(BehaviorWelfareEligibilityFact::Privacy),
        "reproduction" => Welfare(BehaviorWelfareEligibilityFact::Reproduction),
        "rest" => Welfare(BehaviorWelfareEligibilityFact::Rest),
        "social" => Welfare(BehaviorWelfareEligibilityFact::Social),
        "space" => Welfare(BehaviorWelfareEligibilityFact::Space),
        "stimulation" => Welfare(BehaviorWelfareEligibilityFact::Stimulation),
        "thirst" => Welfare(BehaviorWelfareEligibilityFact::Thirst),
        "unbloom" => Welfare(BehaviorWelfareEligibilityFact::Unbloom),
        "inWater" => Spatial(BehaviorSpatialEligibilityFact::InWater),
        "onLand" => Spatial(BehaviorSpatialEligibilityFact::OnLand),
        "inWater_OR_onLand" => Spatial(BehaviorSpatialEligibilityFact::SupportedSurface),
        "isSwimmingOnSurface" => Spatial(BehaviorSpatialEligibilityFact::SwimmingSurface),
        "isSwimmingUnderwater" => Spatial(BehaviorSpatialEligibilityFact::SwimmingUnderwater),
        "depthAboveBottom" => Spatial(BehaviorSpatialEligibilityFact::DepthAboveBottom),
        "depthBelowSurface" => Spatial(BehaviorSpatialEligibilityFact::DepthBelowSurface),
        "shoreDistance" => Spatial(BehaviorSpatialEligibilityFact::ShoreDistance),
        "waterDepth" => Spatial(BehaviorSpatialEligibilityFact::WaterDepth),
        "waterDirtiness" => Spatial(BehaviorSpatialEligibilityFact::WaterDirtiness),
        "inSight" => Spatial(BehaviorSpatialEligibilityFact::InSight),
        "notInSight" => Spatial(BehaviorSpatialEligibilityFact::NotInSight),
        "timeOfDay" => Spatial(BehaviorSpatialEligibilityFact::TimeOfDay),
        "timeOfDayMin" => Spatial(BehaviorSpatialEligibilityFact::TimeOfDayMin),
        "timeOfDayMax" => Spatial(BehaviorSpatialEligibilityFact::TimeOfDayMax),
        "b_InGlacier" => Spatial(BehaviorSpatialEligibilityFact::InGlacier),
        "b_IsElevatedPath" => Spatial(BehaviorSpatialEligibilityFact::ElevatedPath),
        "isFamily" => Relationship(BehaviorRelationshipEligibilityFact::Family),
        "isRelation" => Relationship(BehaviorRelationshipEligibilityFact::Relation),
        "sameSpecies" => Relationship(BehaviorRelationshipEligibilityFact::SameSpecies),
        "hasTrick" => Relationship(BehaviorRelationshipEligibilityFact::HasTrick),
        "biome" => Taxonomy(BehaviorTaxonomyEligibilityFact::Biome),
        "b_Felidae" => Taxonomy(BehaviorTaxonomyEligibilityFact::Felidae),
        "b_MarineAnimal" => Taxonomy(BehaviorTaxonomyEligibilityFact::MarineAnimal),
        "b_SmallPredator" => Taxonomy(BehaviorTaxonomyEligibilityFact::SmallPredator),
        "b_MediumPredator" => Taxonomy(BehaviorTaxonomyEligibilityFact::MediumPredator),
        "b_LargePredator" => Taxonomy(BehaviorTaxonomyEligibilityFact::LargePredator),
        "b_MediumPrey" => Taxonomy(BehaviorTaxonomyEligibilityFact::MediumPrey),
        "b_XLargePredator" => Taxonomy(BehaviorTaxonomyEligibilityFact::ExtraLargePredator),
        "b_CaffeineTrigger" => Condition(BehaviorConditionEligibilityFact::CaffeineTrigger),
        "b_EdDonationBonus" => Condition(BehaviorConditionEligibilityFact::DonationBonus),
        "b_IsWishFountain" => Condition(BehaviorConditionEligibilityFact::WishFountain),
        "b_NoBuyBackGift" => Condition(BehaviorConditionEligibilityFact::NoBuyBackGift),
        "b_NoBuyHeadGift" => Condition(BehaviorConditionEligibilityFact::NoBuyHeadGift),
        "b_NoBuyLHandGift" => Condition(BehaviorConditionEligibilityFact::NoBuyLeftHandGift),
        "b_NoBuyRHandGift" => Condition(BehaviorConditionEligibilityFact::NoBuyRightHandGift),
        "b_NoBuyShirtGift" => Condition(BehaviorConditionEligibilityFact::NoBuyShirtGift),
        "b_NoBuyWaistGift" => Condition(BehaviorConditionEligibilityFact::NoBuyWaistGift),
        "b_NoFossilDig" => Condition(BehaviorConditionEligibilityFact::NoFossilDig),
        "b_ProvidesCover" => Condition(BehaviorConditionEligibilityFact::ProvidesCover),
        "f_BoneLevel" => Condition(BehaviorConditionEligibilityFact::BoneLevel),
        "f_PaintLevel_1" => Condition(BehaviorConditionEligibilityFact::PaintLevel1),
        "f_PaintLevel_2" => Condition(BehaviorConditionEligibilityFact::PaintLevel2),
        "f_PaintLevel_3" => Condition(BehaviorConditionEligibilityFact::PaintLevel3),
        "f_PaintLevel_4" => Condition(BehaviorConditionEligibilityFact::PaintLevel4),
        "f_IceLevel" => Condition(BehaviorConditionEligibilityFact::IceLevel),
        "f_BuildingStrength" => Condition(BehaviorConditionEligibilityFact::BuildingStrength),
        "f_ConstructionLevel" => Condition(BehaviorConditionEligibilityFact::ConstructionLevel),
        "f_Damage" => Condition(BehaviorConditionEligibilityFact::Damage),
        "b_Opened" => Condition(BehaviorConditionEligibilityFact::Opened),
        "f_FenceStrength" => Condition(BehaviorConditionEligibilityFact::FenceStrength),
        "f_FilterDirtyLevel" => Condition(BehaviorConditionEligibilityFact::FilterDirtiness),
        "f_FoodLevel" => Condition(BehaviorConditionEligibilityFact::FoodLevel),
        "f_TrashLevel" => Condition(BehaviorConditionEligibilityFact::TrashLevel),
        "f_departurePoints" => Condition(BehaviorConditionEligibilityFact::DeparturePoints),
        "f_needPointsBad" => Condition(BehaviorConditionEligibilityFact::NeedPointsBad),
        "f_needPointsGood" | "needPointsGood" | "needGoodPoints" => {
            Condition(BehaviorConditionEligibilityFact::NeedPointsGood)
        }
        _ => {
            return Err(invalid_behavior_source_data(format!(
                "unmapped eligibility fact {authored_fact_name}"
            )));
        }
    };
    let (eligibility_comparison, comparison_body) = if matches!(
        eligibility_input,
        Spatial(
            BehaviorSpatialEligibilityFact::InSight | BehaviorSpatialEligibilityFact::NotInSight
        )
    ) {
        // Sight uses a strict radius comparison.
        // Retain the unsquared radius in the typed fact; consumers compare
        // squared planar distances. Prefix expressions are not supported here.
        let radius = match authored_fact_value.trim() {
            "T" => "true",
            "F" => "false",
            radius => radius,
        };
        if !radius.eq_ignore_ascii_case("true")
            && !radius.eq_ignore_ascii_case("false")
            && !radius.parse::<f64>().is_ok_and(f64::is_finite)
        {
            return Err(invalid_behavior_source_data(format!(
                "unsupported sight qualifier radius {authored_fact_value}"
            )));
        }
        (BehaviorEligibilityFactComparison::Less, radius)
    } else {
        split_behavior_eligibility_comparison_prefix(authored_fact_value)
    };
    let eligibility_value_words = comparison_body.split_whitespace().collect::<Vec<_>>();
    let eligibility_value = match eligibility_value_words.as_slice() {
        [authored_scalar] if authored_scalar.eq_ignore_ascii_case("true") => {
            BehaviorEligibilityFactValue::Q16(1 << 16)
        }
        [authored_scalar] if authored_scalar.eq_ignore_ascii_case("false") => {
            BehaviorEligibilityFactValue::Q16(0)
        }
        [authored_scalar_or_symbol] => authored_scalar_or_symbol
            .parse::<f64>()
            .ok()
            .filter(|parsed_scalar| parsed_scalar.is_finite())
            .map(|parsed_scalar| {
                BehaviorEligibilityFactValue::Q16((parsed_scalar * 65_536.0).round() as i32)
            })
            .unwrap_or_else(|| {
                BehaviorEligibilityFactValue::Symbol(AssetId::from_key(
                    &authored_scalar_or_symbol.to_ascii_lowercase(),
                ))
            }),
        [authored_symbol, authored_threshold] => BehaviorEligibilityFactValue::SymbolThreshold {
            symbol: AssetId::from_key(&authored_symbol.to_ascii_lowercase()),
            q16: authored_threshold
                .parse::<f64>()
                .ok()
                .filter(|parsed_threshold| parsed_threshold.is_finite())
                .map(|parsed_threshold| (parsed_threshold * 65_536.0).round() as i32)
                .ok_or_else(|| {
                    invalid_behavior_source_data(format!(
                        "invalid eligibility threshold {comparison_body}"
                    ))
                })?,
        },
        _ => {
            return Err(invalid_behavior_source_data(format!(
                "invalid eligibility value {comparison_body}"
            )));
        }
    };
    Ok(BehaviorCandidateEligibilityFact {
        fact_input: eligibility_input,
        comparison: eligibility_comparison,
        expected_value: eligibility_value,
    })
}

fn split_behavior_eligibility_comparison_prefix(
    authored_eligibility_value: &str,
) -> (BehaviorEligibilityFactComparison, &str) {
    let trimmed_eligibility_value = authored_eligibility_value.trim();
    [
        ("GE", BehaviorEligibilityFactComparison::GreaterOrEqual),
        ("LE", BehaviorEligibilityFactComparison::LessOrEqual),
        ("GT", BehaviorEligibilityFactComparison::Greater),
        ("LT", BehaviorEligibilityFactComparison::Less),
        ("NE", BehaviorEligibilityFactComparison::NotEqual),
        ("EQ", BehaviorEligibilityFactComparison::Equal),
        ("E", BehaviorEligibilityFactComparison::Equal),
        ("L", BehaviorEligibilityFactComparison::Less),
        ("G", BehaviorEligibilityFactComparison::Greater),
    ]
    .into_iter()
    .find_map(|(comparison_prefix, eligibility_comparison)| {
        trimmed_eligibility_value
            .strip_prefix(comparison_prefix)
            .map(|comparison_body| (eligibility_comparison, comparison_body.trim()))
    })
    .unwrap_or((
        BehaviorEligibilityFactComparison::Equal,
        trimmed_eligibility_value,
    ))
}
