use std::{io, sync::Arc};

use openzt2_game_data::behavior::action::feedback::BehaviorFeedbackEntry;

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};

const SHOW_POLICY_SOURCE: &str = "AI/ZTAI.xml";

#[derive(Asset, TypePath)]
pub(super) struct AuthoredShowRewards {
    pub(super) failure_multiplier: f32,
    pub(super) success_multiplier: f32,
    pub(super) critical_multiplier: f32,
    pub(super) failures_to_ignore: u16,
    pub(super) trick_token_timeout_seconds: f32,
    pub(super) score_fraction: f32,
    pub(super) popularity: TrickPopularityPolicy,
    default: Vec<DonationFeedback>,
    cancelled: Vec<DonationFeedback>,
    bands: Vec<DonationBand>,
}

pub(super) struct TrickPopularityPolicy {
    pub(super) maximum: f32,
    pub(super) minimum: f32,
    pub(super) decrement_multiplier: f32,
    pub(super) growth_per_second: f32,
}

struct DonationBand {
    minimum_tricks: u16,
    minimum_score: f32,
    feedback: Vec<DonationFeedback>,
}

struct DonationFeedback {
    weight: f32,
    range: [f32; 2],
    favourite_bonus: f32,
    show_need_update: f32,
    presentation: Arc<[BehaviorFeedbackEntry]>,
}

pub(super) struct AudienceFeedback<'a> {
    pub(super) dollars: f64,
    pub(super) need_update: f32,
    pub(super) presentation: &'a Arc<[BehaviorFeedbackEntry]>,
}

impl AuthoredShowRewards {
    pub(super) fn audience_feedback(
        &self,
        completed: Option<(u16, f32)>,
        favourite: bool,
        mut draw: impl FnMut() -> u32,
    ) -> Option<AudienceFeedback<'_>> {
        let feedback = if let Some((completed_tricks, score)) = completed {
            self.bands
                .iter()
                .filter(|band| {
                    band.minimum_tricks <= completed_tricks && band.minimum_score <= score
                })
                .max_by(|left, right| {
                    left.minimum_tricks
                        .cmp(&right.minimum_tricks)
                        .then_with(|| left.minimum_score.total_cmp(&right.minimum_score))
                })
                .map_or(self.default.as_slice(), |band| band.feedback.as_slice())
        } else {
            self.cancelled.as_slice()
        };
        if feedback.is_empty() {
            return None;
        }
        let unit_draw = |value| f64::from(value) / f64::from(u32::MAX);
        let total_weight: f64 = feedback.iter().map(|row| f64::from(row.weight)).sum();
        let mut choice = unit_draw(draw()) * total_weight;
        for row in feedback {
            choice -= f64::from(row.weight);
            if row.weight > 0.0 && choice <= 0.0 {
                let [minimum, maximum] = row.range.map(f64::from);
                let bonus = if favourite {
                    f64::from(row.favourite_bonus)
                } else {
                    0.0
                };
                return Some(AudienceFeedback {
                    dollars: minimum + (maximum - minimum) * unit_draw(draw()) + bonus,
                    need_update: row.show_need_update,
                    presentation: &row.presentation,
                });
            }
        }
        None
    }
}

#[derive(Resource)]
pub(super) struct AuthoredShowRewardsSource(pub(super) Handle<AuthoredShowRewards>);

pub(super) fn load_authored_show_rewards(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(AuthoredShowRewardsSource(assets.load(SHOW_POLICY_SOURCE)));
}

#[derive(Default, TypePath)]
pub(super) struct AuthoredShowRewardsLoader;

impl AssetLoader for AuthoredShowRewardsLoader {
    type Asset = AuthoredShowRewards;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<AuthoredShowRewards> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = AssetPath::new(context.path().path().to_string_lossy());
        let source = parse_blue_fang_source_document(path, &bytes).map_err(io::Error::other)?;
        let show = child(&source.root, "ZTAIShowMgr")?;
        let tricks = child(&source.root, "ZTAITrickMgr")?;
        let scores = child(tricks, "TrickScores")?;
        let mut bands = Vec::new();
        for row in child(show, "ScoreTable")?.element_children() {
            let minimum_tricks = value(row, "MinTricks")?;
            for entry in row.element_children() {
                bands.push(DonationBand {
                    minimum_tricks,
                    minimum_score: value(entry, "MinScore")?,
                    feedback: donation_feedback(entry)?,
                });
            }
        }
        let optional_number = |node: Option<&OrderedSourceDocumentNode>, name, default| {
            node.and_then(|node| value::<f32>(node, name).ok())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
        };
        let popularity = child(&source.root, "ZTAITrickPopularityMgr").ok();
        let maximum = optional_number(popularity, "MaxPopularity", 100.0);
        Ok(AuthoredShowRewards {
            score_fraction: optional_number(
                child(show, "General").ok(),
                "PercentTricksToScore",
                0.5,
            ),
            popularity: TrickPopularityPolicy {
                maximum,
                minimum: optional_number(popularity, "MinPopularity", 10.0).min(maximum),
                decrement_multiplier: optional_number(popularity, "DecrementMultiplier", 1.0),
                growth_per_second: optional_number(popularity, "PopularityPerSecond", 0.0),
            },
            failure_multiplier: value(scores, "FailureMultiplier")?,
            success_multiplier: value(scores, "SuccessMultiplier")?,
            critical_multiplier: value(scores, "CriticalSuccessMultiplier")?,
            failures_to_ignore: value(scores, "FailuresToIgnore")?,
            trick_token_timeout_seconds: value(
                child(tricks, "General")?,
                "DefaultTrickTokenTimeout",
            )?,
            default: donation_feedback(child(show, "DefaultFeedback")?)?,
            cancelled: show
                .element_children()
                .find(|node| node.name.eq_ignore_ascii_case("ShowCancelledFeedback"))
                .map(donation_feedback)
                .transpose()?
                .unwrap_or_default(),
            bands,
        })
    }
}

fn child<'a>(
    parent: &'a OrderedSourceDocumentNode,
    name: &str,
) -> io::Result<&'a OrderedSourceDocumentNode> {
    parent
        .element_children()
        .find(|node| node.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("missing show element {name}"),
            )
        })
}

fn value<T: std::str::FromStr>(node: &OrderedSourceDocumentNode, name: &str) -> io::Result<T> {
    node.attribute_named_any(&[name])
        .and_then(parse_blue_fang_source_numeric_lexeme)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid show field {name}"),
            )
        })
}

fn donation_feedback(node: &OrderedSourceDocumentNode) -> io::Result<Vec<DonationFeedback>> {
    node.element_children()
        .filter(|row| row.name.eq_ignore_ascii_case("Feedback"))
        .map(|row| {
            let feedback = DonationFeedback {
                weight: value(row, "Weight")?,
                range: [value(row, "DonateMin")?, value(row, "DonateMax")?],
                favourite_bonus: value(row, "FavoriteAnimalBonus")?,
                show_need_update: value(row, "ShowNeedUpdate").unwrap_or_default(),
                presentation: row
                    .element_children()
                    .filter(|node| node.name.eq_ignore_ascii_case("ZTFeedbackData"))
                    .flat_map(OrderedSourceDocumentNode::element_children)
                    .map(feedback_entry)
                    .collect::<io::Result<Arc<[_]>>>()?,
            };
            if ![
                feedback.weight,
                feedback.range[0],
                feedback.range[1],
                feedback.favourite_bonus,
                feedback.show_need_update,
            ]
            .into_iter()
            .all(f32::is_finite)
                || feedback.weight < 0.0
                || feedback.range[0] > feedback.range[1]
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid show donation interval",
                ));
            }
            Ok(feedback)
        })
        .collect()
}

fn feedback_entry(node: &OrderedSourceDocumentNode) -> io::Result<BehaviorFeedbackEntry> {
    let attribute = |name| node.attribute_named_any(&[name]);
    let string = |name| attribute(name).map(str::to_owned);
    let required = |name| {
        string(name).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("missing show feedback field {name}"),
            )
        })
    };
    let boolean =
        |name| attribute(name).map(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    let flag = |name| boolean(name).unwrap_or_default();
    let seconds = |name| value(node, name).ok();
    let use_entity_name = flag("useEntityName");
    let use_target_name = flag("useTargetName");
    match node.name.as_str() {
        "ZTThoughtInfo" => Ok(BehaviorFeedbackEntry::Thought {
            localization_key: required("locID")?,
            priority: value(node, "priority").ok(),
            timeout_seconds: seconds("timeout"),
            globally_visible: boolean("global"),
            use_entity_name,
            use_target_name,
            use_target_target: flag("useTargetTarget"),
            use_target_object: flag("useTargetObject"),
            use_target_species: flag("useTargetSpecies"),
        }),
        "ZTActionInfo" => Ok(BehaviorFeedbackEntry::Action {
            localization_key: required("locID")?,
            use_entity_name,
            use_target_name,
            use_target_contents: flag("useTargetContents"),
            use_object_name: flag("useObjectName"),
        }),
        "ZTMessageInfo" => Ok(BehaviorFeedbackEntry::Message {
            localization_key: required("locID")?,
            priority: value(node, "priority").ok(),
            timeout_seconds: seconds("timeout"),
            interval_seconds: seconds("interval"),
            tolerance: value(node, "tolerance").ok(),
            globally_visible: boolean("global"),
            use_entity_name,
            use_target_name,
            filter_attribute_expression: string("filterAttribute"),
        }),
        "ZTEmoticonInfo" => Ok(BehaviorFeedbackEntry::Emoticon {
            emoticon_name: required("emoticonName")?,
            emoticon_scale: value(node, "emoticonScale").ok(),
            emoticon_entity_node: string("emoticonEntityNode"),
        }),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown show feedback {}", node.name),
        )),
    }
}
