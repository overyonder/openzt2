//! Parses fossil-placement policies and training definitions.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode, OrderedSourceDocumentSpan,
};
use openzt2_game_data::{
    world_definitions::animal_shows_and_training::{TrickLevel, TrickPrerequisite},
    AssetId,
};
use std::collections::{BTreeMap, BTreeSet};

use super::behavior_auxiliary_source_types::{
    BehaviorAuxiliaryDiagnostic, BehaviorAuxiliaryDiagnosticKind, FossilSonarPolicy,
    LoweredBehaviorAuxiliary, PuzzlePlacementPolicy, SourceTrickDefinition, TrickOutcomeTokens,
};

fn has_element(document: &OrderedSourceDocument, wanted: &str) -> bool {
    fn contains(node: &OrderedSourceDocumentNode, wanted: &str) -> bool {
        node.name.eq_ignore_ascii_case(wanted)
            || node.element_children().any(|child| contains(child, wanted))
    }
    document.root.name.eq_ignore_ascii_case(wanted)
        || document
            .root
            .element_children()
            .any(|child| contains(child, wanted))
}

pub(crate) fn lower_behavior_auxiliary_source_document(
    document: Option<&OrderedSourceDocument>,
    fossil_mode_document: Option<&OrderedSourceDocument>,
    resolve: impl Fn(&str) -> Option<AssetId>,
) -> Result<LoweredBehaviorAuxiliary, Vec<BehaviorAuxiliaryDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut puzzle_placement_policy = None;
    let mut tricks = Vec::new();
    let mut trick_ids = BTreeMap::new();
    let mut emoticons = BTreeMap::new();
    let fossil_sonar = fossil_mode_document.or(document).and_then(|document| {
        parse_fossil_sonar_policy(document).and_then(|result| match result {
            Ok(policy) => Some(policy),
            Err(mut errors) => {
                diagnostics.append(&mut errors);
                None
            }
        })
    });
    if let Some(document) = document {
        match document.root.name.as_str() {
            "BFParticleDictionary" => match parse_emoticon_dictionary(document, &resolve) {
                Ok(parsed) => {
                    for symbol in parsed {
                        if emoticons.insert(symbol.clone(), ()).is_some() {
                            diagnostics.push(diagnostic(
                                document,
                                None,
                                BehaviorAuxiliaryDiagnosticKind::DuplicateDefinition {
                                    kind: "emoticon",
                                    id: AssetId::from_key(&symbol),
                                },
                            ));
                        }
                    }
                }
                Err(mut errors) => diagnostics.append(&mut errors),
            },
            "ZTGestureMgr" => {
                if let Err(mut errors) = validate_gesture_source_policy(document) {
                    diagnostics.append(&mut errors);
                }
            }
            "ZTPuzzleMgr" => {
                // The shipped manager owns the puzzle roots and object sets,
                // while sonar distances are authored on the separate mode
                // document. The live loader registers that explicit dependency;
                // missing mode data is an error, not an empty manager asset.
                match parse_puzzle_placement_policy(document, &resolve, fossil_sonar) {
                    Ok(policy) => puzzle_placement_policy = Some(policy),
                    Err(mut errors) => diagnostics.append(&mut errors),
                }
            }
            "gestures" => {
                validate_gestures(document, &mut diagnostics);
            }
            "Tricks" => match parse_tricks(document, &resolve, tricks.len() as u32) {
                Ok(parsed) => {
                    for trick in parsed {
                        if trick_ids.insert(trick.id.0, document.path.key()).is_some() {
                            diagnostics.push(diagnostic(
                                document,
                                None,
                                BehaviorAuxiliaryDiagnosticKind::DuplicateDefinition {
                                    kind: "trick",
                                    id: trick.id,
                                },
                            ));
                        } else {
                            tricks.push(trick);
                        }
                    }
                }
                Err(mut errors) => diagnostics.append(&mut errors),
            },
            _ if has_element(document, "ZTFossilFindingMode") => {}
            root => diagnostics.push(diagnostic(
                document,
                Some(document.root.span),
                BehaviorAuxiliaryDiagnosticKind::UnsupportedRoot {
                    root: root.to_owned(),
                },
            )),
        }
    }
    resolve_trick_defaults(&mut tricks, &mut diagnostics);
    if diagnostics.is_empty() {
        // Runtime lookup order is by stable ID. `sort_key` and source ordinal
        // remain available to compile the separate per-species display order.
        tricks.sort_unstable_by_key(|trick| trick.id.0);
        Ok(LoweredBehaviorAuxiliary {
            puzzle_placement_policy,
            tricks,
        })
    } else {
        diagnostics.sort_unstable_by(|left, right| {
            left.virtual_path.cmp(&right.virtual_path).then_with(|| {
                left.span
                    .map(|span| span.start)
                    .cmp(&right.span.map(|span| span.start))
            })
        });
        Err(diagnostics)
    }
}
fn parse_emoticon_dictionary(
    document: &OrderedSourceDocument,
    resolve: &impl Fn(&str) -> Option<AssetId>,
) -> Result<BTreeSet<String>, Vec<BehaviorAuxiliaryDiagnostic>> {
    let mut definitions = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for entry in document
        .root
        .element_children()
        .filter(|entry| !entry.name.eq_ignore_ascii_case("DictionaryDir"))
    {
        let particles = entry
            .element_children()
            .filter(|node| node.name.eq_ignore_ascii_case("Conditions"))
            .flat_map(OrderedSourceDocumentNode::element_children)
            .filter(|node| node.name.eq_ignore_ascii_case("BFParticleDefinition"))
            .collect::<Vec<_>>();
        let [particle] = particles.as_slice() else {
            diagnostics.push(diagnostic(
                document,
                Some(entry.span),
                BehaviorAuxiliaryDiagnosticKind::InvalidField {
                    owner: entry.name.to_string(),
                    field: "Conditions/BFParticleDefinition",
                    value: format!("{} definitions", particles.len()),
                },
            ));
            continue;
        };
        let Some(path) = particle.attribute("xmlFile") else {
            diagnostics.push(missing(
                document,
                Some(particle.span),
                &entry.name,
                "xmlFile",
            ));
            continue;
        };
        match particle.attribute("removeTime") {
            Some(value) => match value.parse::<f64>() {
                Ok(seconds) if seconds.is_finite() && seconds > 0.0 => {}
                _ => {
                    diagnostics.push(diagnostic(
                        document,
                        Some(particle.span),
                        BehaviorAuxiliaryDiagnosticKind::InvalidField {
                            owner: entry.name.to_string(),
                            field: "removeTime",
                            value: value.to_owned(),
                        },
                    ));
                    continue;
                }
            },
            None => {
                diagnostics.push(missing(
                    document,
                    Some(particle.span),
                    &entry.name,
                    "removeTime",
                ));
                continue;
            }
        }
        let _ = resolve(path).unwrap_or_else(|| AssetId::from_virtual_path(path));
        definitions.insert(entry.name.to_ascii_lowercase());
    }
    if diagnostics.is_empty() {
        Ok(definitions)
    } else {
        Err(diagnostics)
    }
}
fn validate_gesture_source_policy(
    document: &OrderedSourceDocument,
) -> Result<(), Vec<BehaviorAuxiliaryDiagnostic>> {
    let Some(editing) = document
        .root
        .element_children()
        .find(|node| node.name == "gestureEditing")
    else {
        return Err(vec![missing(
            document,
            None,
            "ZTGestureMgr",
            "gestureEditing",
        )]);
    };
    let mut diagnostics = reject_children(
        document,
        &document.root.name,
        document.root.element_children(),
        &["gestureEditing"],
    );
    let directory = required(document, editing, "directory", &mut diagnostics);
    let editor_file = required(document, editing, "fileToEdit", &mut diagnostics);
    if diagnostics.is_empty() {
        debug_assert!(directory.is_some() && editor_file.is_some());
        Ok(())
    } else {
        Err(diagnostics)
    }
}
fn parse_puzzle_placement_policy(
    document: &OrderedSourceDocument,
    resolve: &impl Fn(&str) -> Option<AssetId>,
    fossil_sonar: Option<FossilSonarPolicy>,
) -> Result<PuzzlePlacementPolicy, Vec<BehaviorAuxiliaryDiagnostic>> {
    let root = &document.root;
    let mut diagnostics = Vec::new();
    let puzzle_root = root.attribute("puzzleRoot").map(str::to_owned).or_else(|| {
        diagnostics.push(missing(
            document,
            Some(root.span),
            "ZTPuzzleMgr",
            "puzzleRoot",
        ));
        None
    });
    let entity_root = root.attribute("entityRoot").map(str::to_owned).or_else(|| {
        diagnostics.push(missing(
            document,
            Some(root.span),
            "ZTPuzzleMgr",
            "entityRoot",
        ));
        None
    });
    let placeable_objects = root
        .attribute("placeableObjects")
        .map(|value| asset_list(value, resolve))
        .or_else(|| {
            diagnostics.push(missing(
                document,
                Some(root.span),
                "ZTPuzzleMgr",
                "placeableObjects",
            ));
            None
        });
    let non_placeable_objects = root
        .attribute("nonPlaceableObjects")
        .map(|value| asset_list(value, resolve))
        .or_else(|| {
            diagnostics.push(missing(
                document,
                Some(root.span),
                "ZTPuzzleMgr",
                "nonPlaceableObjects",
            ));
            None
        });
    let fossil_sonar = fossil_sonar.or_else(|| {
        diagnostics.push(missing(
            document,
            Some(root.span),
            "ZTFossilFindingMode",
            "minDistance/maxDistance/sonarViewCone/digDistance",
        ));
        None
    });
    if root.element_children().next().is_some() {
        diagnostics.extend(reject_children(
            document,
            "ZTPuzzleMgr",
            root.element_children(),
            &[],
        ));
    }
    if diagnostics.is_empty() {
        Ok(PuzzlePlacementPolicy {
            puzzle_root: puzzle_root.unwrap(),
            entity_root: entity_root.unwrap(),
            placeable_objects: placeable_objects.unwrap(),
            non_placeable_objects: non_placeable_objects.unwrap(),
            minimum_sonar_distance_squared: fossil_sonar.unwrap().minimum_distance_squared,
            maximum_sonar_distance_squared: fossil_sonar.unwrap().maximum_distance_squared,
            minimum_sonar_view_dot: fossil_sonar.unwrap().minimum_view_dot,
            dig_distance_m: fossil_sonar.unwrap().dig_distance_m,
        })
    } else {
        Err(diagnostics)
    }
}
fn parse_fossil_sonar_policy(
    document: &OrderedSourceDocument,
) -> Option<Result<FossilSonarPolicy, Vec<BehaviorAuxiliaryDiagnostic>>> {
    fn descendant<'a>(
        node: &'a OrderedSourceDocumentNode,
        name: &str,
    ) -> Option<&'a OrderedSourceDocumentNode> {
        (node.name == name).then_some(node).or_else(|| {
            node.element_children()
                .find_map(|child| descendant(child, name))
        })
    }
    let node = document
        .root
        .element_children()
        .find_map(|child| descendant(child, "ZTFossilFindingMode"))?;
    let mut diagnostics = Vec::new();
    let mut number = |field: &'static str| match node.attribute(field) {
        Some(value) => match value.parse::<f32>() {
            Ok(value) if value.is_finite() => Some(value),
            _ => {
                diagnostics.push(BehaviorAuxiliaryDiagnostic {
                    virtual_path: document.path.key(),
                    span: Some(node.span),
                    kind: BehaviorAuxiliaryDiagnosticKind::InvalidField {
                        owner: "ZTFossilFindingMode".to_owned(),
                        field,
                        value: value.to_owned(),
                    },
                });
                None
            }
        },
        None => {
            diagnostics.push(missing(
                document,
                Some(node.span),
                "ZTFossilFindingMode",
                field,
            ));
            None
        }
    };
    let minimum = number("minDistance");
    let maximum = number("maxDistance");
    let cone_degrees = number("sonarViewCone");
    let dig_distance = number("digDistance");
    Some(if diagnostics.is_empty() {
        Ok(FossilSonarPolicy {
            dig_distance_m: dig_distance.unwrap(),
            minimum_distance_squared: minimum.unwrap().powi(2),
            maximum_distance_squared: maximum.unwrap().powi(2),
            minimum_view_dot: (cone_degrees.unwrap().to_radians() * 0.5).cos(),
        })
    } else {
        Err(diagnostics)
    })
}
fn parse_tricks(
    document: &OrderedSourceDocument,
    resolve: &impl Fn(&str) -> Option<AssetId>,
    source_base: u32,
) -> Result<Vec<SourceTrickDefinition>, Vec<BehaviorAuxiliaryDiagnostic>> {
    let mut diagnostics = reject_children(
        document,
        "Tricks",
        document.root.element_children(),
        &["ZTAITrickData"],
    );
    let mut tricks = Vec::new();
    for (ordinal, node) in document
        .root
        .element_children()
        .filter(|node| node.name == "ZTAITrickData")
        .enumerate()
    {
        if let Some(trick) = parse_trick(
            document,
            node,
            resolve,
            source_base + ordinal as u32,
            &mut diagnostics,
        ) {
            tricks.push(trick);
        }
    }
    if diagnostics.is_empty() {
        Ok(tricks)
    } else {
        Err(diagnostics)
    }
}
fn parse_trick(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    resolve: &impl Fn(&str) -> Option<AssetId>,
    source_ordinal: u32,
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) -> Option<SourceTrickDefinition> {
    diagnostics.extend(reject_children(
        document,
        "ZTAITrickData",
        node.element_children(),
        &[
            "Data",
            "Animals",
            "Prerequisite",
            "ZTAITrickTargetData",
            "Levels",
            "Failure",
            "Success",
            "Critical",
        ],
    ));
    let data = child(node, "Data").or_else(|| {
        diagnostics.push(missing(document, Some(node.span), "ZTAITrickData", "Data"));
        None
    })?;
    let owner = data.attribute("name").unwrap_or("<unnamed trick>");
    let id = required(document, data, "name", diagnostics).map(|value| asset(value, resolve))?;
    let localized_name =
        required(document, data, "locname", diagnostics).map(|value| asset(value, resolve))?;
    let icon = required(document, data, "icon", diagnostics).map(|value| asset(value, resolve))?;
    let difficulty = number::<u16>(document, data, owner, "Difficulty", diagnostics)?;
    let popularity_q16 = fixed_q16(document, data, owner, "PopularityMult", diagnostics)?;
    let animals = child(node, "Animals").or_else(|| {
        diagnostics.push(missing(document, Some(node.span), owner, "Animals"));
        None
    })?;
    let eligible_species = animals
        .element_children()
        .map(|animal| asset(&animal.name, resolve))
        .collect::<Vec<_>>();
    if eligible_species.is_empty() {
        diagnostics.push(missing(
            document,
            Some(animals.span),
            owner,
            "eligible animal",
        ));
    }
    let prerequisite = child(node, "Prerequisite").and_then(|prerequisite| {
        Some(TrickPrerequisite {
            trick: required(document, prerequisite, "name", diagnostics)
                .map(|value| asset(value, resolve))?,
            minimum_score: number(document, prerequisite, owner, "minScore", diagnostics)?,
        })
    });
    let target_node = child(node, "ZTAITrickTargetData").or_else(|| {
        diagnostics.push(missing(
            document,
            Some(node.span),
            owner,
            "ZTAITrickTargetData",
        ));
        None
    })?;
    let target =
        required(document, target_node, "Prop", diagnostics).map(|value| asset(value, resolve))?;
    let defaults_from = data
        .attribute("defaultTrick")
        .map(|value| asset(value, resolve));
    let levels = child(node, "Levels")
        .map(|levels| {
            levels
                .element_children()
                .filter(|level| level.name == "ZTAITrickLevel")
                .filter_map(|level| parse_trick_level(document, level, owner, resolve, diagnostics))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if levels.is_empty() && defaults_from.is_none() {
        diagnostics.push(missing(document, Some(node.span), owner, "Levels"));
    }
    let outcome_tokens = match (
        optional_outcome_token(document, node, "Failure", resolve, diagnostics),
        optional_outcome_token(document, node, "Success", resolve, diagnostics),
        optional_outcome_token(document, node, "Critical", resolve, diagnostics),
    ) {
        (Some(failure), Some(success), Some(critical)) => Some(TrickOutcomeTokens {
            failure,
            success,
            critical,
        }),
        (None, None, None) if defaults_from.is_some() => None,
        _ => {
            diagnostics.push(missing(
                document,
                Some(node.span),
                owner,
                "complete outcome token set",
            ));
            None
        }
    };
    Some(SourceTrickDefinition {
        id,
        localized_name,
        icon,
        difficulty,
        popularity_q16,
        sort_key: data.attribute("sortKey").map(str::to_owned),
        source_ordinal,
        eligible_species,
        defaults_from,
        prerequisite,
        target,
        levels,
        outcome_tokens,
    })
}
fn parse_trick_level(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    owner: &str,
    resolve: &impl Fn(&str) -> Option<AssetId>,
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) -> Option<TrickLevel> {
    Some(TrickLevel {
        minimum_score: number(document, node, owner, "minScore", diagnostics)?,
        failure_percent: percentage(document, node, owner, "chanceFail", diagnostics)?,
        success_percent: percentage(document, node, owner, "chanceSucceed", diagnostics)?,
        critical_percent: percentage(document, node, owner, "chanceCritical", diagnostics)?,
        gesture: node.attribute("gesture").map(|value| asset(value, resolve)),
        learning_delta: if node.attribute("learningDelta").is_some() {
            number(document, node, owner, "learningDelta", diagnostics)?
        } else {
            0
        },
        staff_can_train: boolean_or(document, node, owner, "staffCanTrain", false, diagnostics)?,
        can_perform: boolean_or(document, node, owner, "canPerform", false, diagnostics)?,
    })
}
fn optional_outcome_token(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    kind: &'static str,
    resolve: &impl Fn(&str) -> Option<AssetId>,
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) -> Option<AssetId> {
    child(node, kind)
        .and_then(|outcome| required(document, outcome, "token", diagnostics))
        .map(|value| asset(value, resolve))
}
fn resolve_trick_defaults(
    tricks: &mut [SourceTrickDefinition],
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) {
    let by_id = tricks
        .iter()
        .enumerate()
        .map(|(index, trick)| (trick.id.0, index))
        .collect::<BTreeMap<_, _>>();
    for index in 0..tricks.len() {
        let Some(parent_id) = tricks[index].defaults_from else {
            continue;
        };
        let Some(&parent_index) = by_id.get(&parent_id.0) else {
            diagnostics.push(inheritance_error(tricks[index].id, "missing defaultTrick"));
            continue;
        };
        let inherited_levels = tricks[parent_index].levels.clone();
        let inherited_tokens = tricks[parent_index].outcome_tokens;
        if tricks[index].levels.is_empty() {
            tricks[index].levels = inherited_levels;
        }
        if tricks[index].outcome_tokens.is_none() {
            tricks[index].outcome_tokens = inherited_tokens;
        }
    }
    tricks
        .iter()
        .filter(|trick| trick.levels.is_empty() || trick.outcome_tokens.is_none())
        .for_each(|trick| diagnostics.push(inheritance_error(trick.id, "incomplete defaultTrick")));
}
fn inheritance_error(id: AssetId, detail: &'static str) -> BehaviorAuxiliaryDiagnostic {
    BehaviorAuxiliaryDiagnostic {
        virtual_path: String::new(),
        span: None,
        kind: BehaviorAuxiliaryDiagnosticKind::InvalidField {
            owner: "trick default inheritance".to_owned(),
            field: "defaultTrick",
            value: format!("{detail}: {:02x?}", id.0),
        },
    }
}
fn validate_gestures(
    document: &OrderedSourceDocument,
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) {
    document.root.element_children().for_each(|gesture| {
        if gesture.name != "gesture" {
            diagnostics.push(diagnostic(
                document,
                Some(gesture.span),
                BehaviorAuxiliaryDiagnosticKind::UnsupportedChild {
                    owner: "gestures".to_owned(),
                    child: gesture.name.to_string(),
                },
            ));
            return;
        }
        gesture
            .element_children()
            .for_each(|node| match node.name.as_str() {
                "point" | "event" | "traceScoreDistance" => {}
                child => diagnostics.push(diagnostic(
                    document,
                    Some(node.span),
                    BehaviorAuxiliaryDiagnosticKind::UnsupportedChild {
                        owner: "gesture".to_owned(),
                        child: child.to_owned(),
                    },
                )),
            });
    });
}
fn child<'a>(
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    node.element_children().find(|child| child.name == name)
}
fn required<'a>(
    document: &OrderedSourceDocument,
    node: &'a OrderedSourceDocumentNode,
    field: &'static str,
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) -> Option<&'a str> {
    node.attribute(field).or_else(|| {
        diagnostics.push(missing(document, Some(node.span), &node.name, field));
        None
    })
}
fn number<T: std::str::FromStr>(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    owner: &str,
    field: &'static str,
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) -> Option<T> {
    let value = required(document, node, field, diagnostics)?;
    value.parse().ok().or_else(|| {
        diagnostics.push(invalid(document, node.span, owner, field, value));
        None
    })
}
fn percentage(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    owner: &str,
    field: &'static str,
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) -> Option<u8> {
    let value = number::<u8>(document, node, owner, field, diagnostics)?;
    (value <= 100).then_some(value).or_else(|| {
        diagnostics.push(invalid(
            document,
            node.span,
            owner,
            field,
            &value.to_string(),
        ));
        None
    })
}
fn fixed_q16(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    owner: &str,
    field: &'static str,
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) -> Option<i32> {
    let value = required(document, node, field, diagnostics)?;
    value
        .parse::<f64>()
        .ok()
        .and_then(|value| {
            let scaled = (value * 65_536.0).round();
            (scaled.is_finite() && scaled >= i32::MIN as f64 && scaled <= i32::MAX as f64)
                .then_some(scaled as i32)
        })
        .or_else(|| {
            diagnostics.push(invalid(document, node.span, owner, field, value));
            None
        })
}
fn boolean_or(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    owner: &str,
    field: &'static str,
    default: bool,
    diagnostics: &mut Vec<BehaviorAuxiliaryDiagnostic>,
) -> Option<bool> {
    let Some(value) = node.attribute(field) else {
        return Some(default);
    };
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => {
            diagnostics.push(invalid(document, node.span, owner, field, value));
            None
        }
    }
}
fn asset(value: &str, resolve: &impl Fn(&str) -> Option<AssetId>) -> AssetId {
    resolve(value).unwrap_or_else(|| AssetId::from_key(value))
}
fn asset_list(value: &str, resolve: &impl Fn(&str) -> Option<AssetId>) -> Vec<AssetId> {
    value
        .split_whitespace()
        .map(|value| asset(value, resolve))
        .collect()
}
fn reject_children<'a>(
    document: &OrderedSourceDocument,
    owner: &str,
    children: impl Iterator<Item = &'a OrderedSourceDocumentNode>,
    allowed: &[&str],
) -> Vec<BehaviorAuxiliaryDiagnostic> {
    children
        .filter(|child| !allowed.contains(&child.name.as_str()))
        .map(|child| {
            diagnostic(
                document,
                Some(child.span),
                BehaviorAuxiliaryDiagnosticKind::UnsupportedChild {
                    owner: owner.to_owned(),
                    child: child.name.to_string(),
                },
            )
        })
        .collect()
}
fn missing(
    document: &OrderedSourceDocument,
    span: Option<OrderedSourceDocumentSpan>,
    owner: &str,
    field: &'static str,
) -> BehaviorAuxiliaryDiagnostic {
    diagnostic(
        document,
        span,
        BehaviorAuxiliaryDiagnosticKind::MissingField {
            owner: owner.to_owned(),
            field,
        },
    )
}
fn invalid(
    document: &OrderedSourceDocument,
    span: OrderedSourceDocumentSpan,
    owner: &str,
    field: &'static str,
    value: &str,
) -> BehaviorAuxiliaryDiagnostic {
    diagnostic(
        document,
        Some(span),
        BehaviorAuxiliaryDiagnosticKind::InvalidField {
            owner: owner.to_owned(),
            field,
            value: value.to_owned(),
        },
    )
}
fn diagnostic(
    document: &OrderedSourceDocument,
    span: Option<OrderedSourceDocumentSpan>,
    kind: BehaviorAuxiliaryDiagnosticKind,
) -> BehaviorAuxiliaryDiagnostic {
    BehaviorAuxiliaryDiagnostic {
        virtual_path: document.path.key(),
        span,
        kind,
    }
}
