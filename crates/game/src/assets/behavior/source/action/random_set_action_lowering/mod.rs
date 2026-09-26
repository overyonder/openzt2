//! Authored weighted behavior-set choices; entries retain their source order.

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_boolean_attribute_or_false, read_optional_float_attribute},
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};
use std::io;

pub(super) fn lower_random_set_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if !matches!(node.name.as_str(), "BFBehRandomSet" | "BFBehAnimateRandom") {
        return Ok(None);
    }
    let animation = node.name == "BFBehAnimateRandom";
    let collection = if animation {
        "randomAnims"
    } else {
        "randomSets"
    };
    if node
        .attributes
        .iter()
        .any(|attribute| !matches!(attribute.name(), "loopFlag" | "minPlays" | "maxPlays"))
        || node
            .element_children()
            .any(|child| child.name != collection)
    {
        return Err(invalid_behavior_source_data(
            "BFBehRandomSet has unmapped context or runtime-state attributes",
        ));
    }
    let count = |name| -> io::Result<u32> {
        let value = node
            .attribute(name)
            .unwrap_or("1")
            .parse::<i32>()
            .map_err(|_| invalid_behavior_source_data("BFBehRandomSet has invalid play count"))?;
        u32::try_from(value)
            .map_err(|_| invalid_behavior_source_data("BFBehRandomSet has negative play count"))
    };
    let minimum_plays = count("minPlays")?;
    let maximum_plays = count("maxPlays")?;
    if maximum_plays < minimum_plays {
        return Err(invalid_behavior_source_data(
            "BFBehRandomSet maximum is below minimum",
        ));
    }
    let weighted_sets = node
        .element_children()
        .filter(|child| child.name == collection)
        .flat_map(OrderedSourceDocumentNode::element_children)
        .map(|entry| {
            if entry
                .attributes
                .iter()
                .any(|attribute| attribute.name() != "weight")
                || entry.element_children().next().is_some()
            {
                return Err(invalid_behavior_source_data(
                    "BFBehRandomSet choice has unmapped policy",
                ));
            }
            let weight = read_optional_float_attribute(entry, "weight")?.unwrap_or(0.0);
            if !weight.is_finite() || weight < 0.0 {
                return Err(invalid_behavior_source_data(
                    "BFBehRandomSet weight must be finite and nonnegative",
                ));
            }
            Ok((entry.name.clone(), weight))
        })
        .collect::<io::Result<Vec<_>>>()?;
    if !weighted_sets
        .iter()
        .map(|(_, weight)| weight)
        .sum::<f32>()
        .is_finite()
    {
        return Err(invalid_behavior_source_data(
            "BFBehRandomSet total weight overflows",
        ));
    }
    let looping = read_boolean_attribute_or_false(node, "loopFlag")?;
    if animation {
        return Ok(Some(BehaviorAction::RandomAnimation {
            weighted_clips: weighted_sets
                .into_iter()
                .map(|(name, weight)| (name.to_string(), weight))
                .collect(),
            minimum_plays,
            maximum_plays,
            looping,
        }));
    }
    Ok(Some(BehaviorAction::RandomSet {
        weighted_sets: weighted_sets
            .into_iter()
            .map(|(name, weight)| (AssetId::from_key(&name.to_ascii_lowercase()), weight))
            .collect(),
        minimum_plays,
        maximum_plays,
        looping,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn random_animation_preserves_clip_spelling_and_native_play_policy() {
        let document = parse_blue_fang_source_document(AssetPath::new("random.beh"),
            br#"<behaviors><BFBehAnimateRandom minPlays="2" maxPlays="4" loopFlag="true"><randomAnims><Stand_Idle weight="70"/><Stand_Look weight="30"/></randomAnims></BFBehAnimateRandom></behaviors>"#).expect("source");
        let action =
            lower_random_set_action(document.root.element_children().next().expect("action"))
                .expect("lower")
                .expect("random");
        let BehaviorAction::RandomAnimation {
            weighted_clips,
            minimum_plays,
            maximum_plays,
            looping,
        } = action
        else {
            panic!("random animation")
        };
        assert_eq!(
            weighted_clips,
            vec![
                ("Stand_Idle".to_owned(), 70.0),
                ("Stand_Look".to_owned(), 30.0)
            ]
        );
        assert_eq!((minimum_plays, maximum_plays, looping), (2, 4, true));
    }

    #[test]
    fn weighted_choices_retain_authored_order_and_play_bounds() {
        let document = parse_blue_fang_source_document(
            AssetPath::new("random.beh"),
            br#"<behaviors><BFBehRandomSet minPlays="2" maxPlays="4"><randomSets><Wander weight="70"/><FlyCall weight="30"/></randomSets></BFBehRandomSet></behaviors>"#,
        ).expect("valid source");
        let action =
            lower_random_set_action(document.root.element_children().next().expect("action"))
                .expect("lowered action")
                .expect("random set");
        let BehaviorAction::RandomSet {
            weighted_sets,
            minimum_plays,
            maximum_plays,
            looping,
        } = action
        else {
            panic!("wrong action");
        };
        assert_eq!(minimum_plays, 2);
        assert_eq!(maximum_plays, 4);
        assert!(!looping);
        assert_eq!(
            weighted_sets,
            vec![
                (AssetId::from_key("wander"), 70.0),
                (AssetId::from_key("flycall"), 30.0)
            ]
        );
    }
}
