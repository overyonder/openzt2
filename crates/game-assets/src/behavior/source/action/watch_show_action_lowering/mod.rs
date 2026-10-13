use std::io;

use openzt2_game_data::{
    behavior::{
        action::watch_show::{BehaviorWatchShowAction, ShowEventReaction, ShowEventScoring},
        action_record::BehaviorAction,
    },
    AssetId,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{invalid_behavior_source_data, source_value_reading::convert_authored_scalar_to_q16};

pub(super) fn lower_watch_show_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "ZTBehWatchShow" {
        return Ok(None);
    }
    let children: Vec<_> = node.element_children().collect();
    let [scoring] = children.as_slice() else {
        return Err(invalid_behavior_source_data(
            "watch show requires EventScoring",
        ));
    };
    if !node.attributes.is_empty()
        || scoring.name != "EventScoring"
        || !scoring.attributes.is_empty()
    {
        return Err(invalid_behavior_source_data("unmapped watch show policy"));
    }
    let mut events = Vec::new();
    for source in scoring.element_children() {
        let event = AssetId::from_key(&source.name.to_ascii_lowercase());
        if events
            .iter()
            .any(|row: &ShowEventScoring| row.event == event)
            || source.attributes.iter().any(|attribute| {
                !matches!(
                    attribute.name(),
                    "Priority"
                        | "BaseScore"
                        | "FuzzyFactor"
                        | "FavoriteAnimalBonus"
                        | "TrickPopularityMult"
                        | "ShowQualityMult"
                        | "PerPrevious"
                        | "IgnorePrevious"
                )
            })
        {
            return Err(invalid_behavior_source_data(
                "unmapped or duplicate show event policy",
            ));
        }
        let number = |name| {
            source
                .attribute(name)
                .map(convert_authored_scalar_to_q16)
                .transpose()
        };
        let mut reactions = Vec::new();
        for reaction in source.element_children() {
            if reaction.name != "Reaction"
                || reaction.element_children().next().is_some()
                || reaction
                    .attributes
                    .iter()
                    .any(|attribute| !matches!(attribute.name(), "MinScore" | "BehSet"))
            {
                return Err(invalid_behavior_source_data(
                    "unmapped show reaction policy",
                ));
            }
            let minimum_score_q16 = reaction
                .attribute("MinScore")
                .ok_or_else(|| invalid_behavior_source_data("show reaction has no MinScore"))
                .and_then(convert_authored_scalar_to_q16)?;
            let behavior_set = reaction
                .attribute("BehSet")
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| invalid_behavior_source_data("show reaction has no BehSet"))?;
            reactions.push(ShowEventReaction {
                minimum_score_q16,
                behavior_set: AssetId::from_key(&behavior_set.to_ascii_lowercase()),
            });
        }
        if reactions.is_empty() {
            return Err(invalid_behavior_source_data("show event has no reactions"));
        }
        events.push(ShowEventScoring {
            event,
            priority_q16: number("Priority")?,
            base_score_q16: number("BaseScore")?,
            fuzzy_factor_q16: number("FuzzyFactor")?,
            favorite_animal_bonus_q16: number("FavoriteAnimalBonus")?,
            trick_popularity_multiplier_q16: number("TrickPopularityMult")?,
            show_quality_multiplier_q16: number("ShowQualityMult")?,
            per_previous_q16: number("PerPrevious")?,
            ignore_previous_q16: number("IgnorePrevious")?,
            reactions,
        });
    }
    if events.is_empty() {
        return Err(invalid_behavior_source_data(
            "watch show has no event policies",
        ));
    }
    Ok(Some(BehaviorAction::WatchShow(BehaviorWatchShowAction {
        events,
    })))
}
