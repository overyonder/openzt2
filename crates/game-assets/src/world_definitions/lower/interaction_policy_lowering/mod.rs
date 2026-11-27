use openzt2_game_data::{
    world_definitions::{
        guest_simulation_definitions::GuestDirectionalLocomotionClip,
        staff_management::StaffStateVariable,
        world_objects::{WorldObjectCollisionAttributeScore, WorldObjectCollisionScores},
    },
    AssetId,
};
use std::{collections::BTreeMap, time::Duration};

use crate::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    ordered_source_document_types::OrderedSourceDocumentNode,
    resolved_source_record_index::{BindError, RecordView},
};

use super::{
    super::interaction_policies::*,
    source_element_tree_search::authored_type_family_elements_named,
    world_definition_source_value_reading_and_conversion::{element_bool, id},
};

fn field<'a>(node: &'a OrderedSourceDocumentNode, name: &str) -> Option<&'a str> {
    node.attribute_named_any(&[name])
        .filter(|value| !value.trim().is_empty())
}

fn number(record: &RecordView<'_, '_>, name: &str, value: &str) -> Result<f32, BindError> {
    parse_blue_fang_source_numeric_lexeme(value)
        .filter(|value: &f32| value.is_finite())
        .ok_or_else(|| BindError::record(record, format!("invalid {name} value {value:?}")))
}

fn read_number(
    record: &RecordView<'_, '_>,
    node: &OrderedSourceDocumentNode,
    name: &str,
) -> Result<Option<f32>, BindError> {
    field(node, name)
        .map(|value| number(record, name, value))
        .transpose()
}

pub(super) fn locomotion_directional_clips(
    record: &RecordView<'_, '_>,
) -> Result<Vec<GuestDirectionalLocomotionClip>, BindError> {
    use super::source_element_tree_search::find_descendant;
    let mut clips = BTreeMap::new();
    for named in authored_type_family_elements_named(record, "BFNamedBinder")
        .into_iter()
        .rev()
    {
        let (Some(binder), Some(locomotion)) = (
            field(named, "binderName").map(id),
            find_descendant(named, "BFLocoAnimate"),
        ) else {
            continue;
        };
        for row in locomotion.element_children() {
            let speed = id(row.name.as_str());
            for child in row.element_children() {
                let direction = id(child.name.as_str());
                let key = (binder, speed, direction);
                let previous: Option<&GuestDirectionalLocomotionClip> = clips.get(&key);
                let Some(clip) =
                    field(child, "name").or_else(|| previous.map(|clip| clip.clip.as_str()))
                else {
                    continue;
                };
                clips.insert(
                    key,
                    GuestDirectionalLocomotionClip {
                        binder,
                        speed,
                        direction,
                        clip: clip.to_owned(),
                        wag: read_number(record, child, "wag")?
                            .or_else(|| previous.and_then(|clip| clip.wag)),
                    },
                );
            }
        }
    }
    Ok(clips.into_values().collect())
}

pub(super) fn collision_scores(
    record: &RecordView<'_, '_>,
) -> Result<Option<WorldObjectCollisionScores>, BindError> {
    let nodes = authored_type_family_elements_named(record, "BFGCollisionData");
    let score = |value: &str| {
        value
            .trim()
            .parse::<i32>()
            .map_err(|error| BindError::record(record, format!("invalid collision score: {error}")))
    };
    let base = nodes
        .iter()
        .find_map(|node| field(node, "score"))
        .map(score)
        .transpose()?;
    let mut by_type = BTreeMap::new();
    let mut by_attribute = Vec::<WorldObjectCollisionAttributeScore>::new();
    for node in nodes {
        for child in node.element_children() {
            if child.name.eq_ignore_ascii_case("typeScores") {
                for (name, value) in child.attributes() {
                    if let std::collections::btree_map::Entry::Vacant(row) = by_type.entry(id(name))
                    {
                        row.insert(score(value)?);
                    }
                }
            } else if child.name.eq_ignore_ascii_case("typeAttrScores") {
                for value in child.element_children() {
                    let (Some(property), Some(raw_score)) =
                        (field(value, "attr"), field(value, "score"))
                    else {
                        continue;
                    };
                    let (subject_type, property) = (id(&value.name), id(property));
                    if by_attribute
                        .iter()
                        .any(|row| row.subject_type == subject_type && row.property == property)
                    {
                        continue;
                    }
                    by_attribute.push(WorldObjectCollisionAttributeScore {
                        subject_type,
                        property,
                        score: score(raw_score)?,
                        always_validate: element_bool(&value, &["alwaysValidate"], false)?,
                    });
                }
            }
        }
    }
    Ok(
        (base.is_some() || !by_type.is_empty() || !by_attribute.is_empty()).then(|| {
            WorldObjectCollisionScores {
                base,
                by_type: by_type.into_iter().collect(),
                by_attribute,
            }
        }),
    )
}

pub(super) fn staff_interactions(
    record: &RecordView<'_, '_>,
) -> Result<Vec<StaffInteraction>, BindError> {
    let mut rows = Vec::<(String, BTreeMap<String, String>)>::new();
    let groups = authored_type_family_elements_named(record, "ZTUserStaffAction");
    for group in groups.into_iter().rev() {
        for node in group.element_children() {
            let kind = node.name.to_ascii_lowercase();
            let property = field(node, "name");
            let existing = rows.iter_mut().find(|(candidate, attributes)| {
                *candidate == kind
                    && property.is_none_or(|key| {
                        attributes
                            .get("name")
                            .is_some_and(|value| key.eq_ignore_ascii_case(value))
                    })
            });
            let attributes = node
                .attributes()
                .map(|(name, value)| (name.to_ascii_lowercase(), value.to_owned()));
            if let Some((_, current)) = existing {
                current.extend(attributes);
            } else {
                rows.push((kind, attributes.collect()));
            }
        }
    }
    rows.into_iter()
        .map(|(kind, attributes)| {
            let get = |key| {
                attributes
                    .get(key)
                    .map(String::as_str)
                    .filter(|value| !value.trim().is_empty())
            };
            let float = |key| {
                get(key)
                    .map(|value| number(record, key, value))
                    .transpose()
                    .map(Option::unwrap_or_default)
            };
            let flag = |key, default| {
                get(key).map_or(default, |value| {
                    value.eq_ignore_ascii_case("true") || value == "1"
                })
            };
            let script = |key| {
                get(key)
                    .map(|value| {
                        let mut parts = value.splitn(3, ':');
                        let (Some(context), Some(path), Some(entry)) =
                            (parts.next(), parts.next(), parts.next())
                        else {
                            return Err(BindError::record(
                                record,
                                format!("invalid {key} script reference"),
                            ));
                        };
                        Ok(StaffInteractionScript {
                            context: id(context),
                            path: path.to_owned(),
                            entry: entry.to_owned(),
                        })
                    })
                    .transpose()
            };
            Ok(StaffInteraction {
                kind: id(&kind),
                property: get("name").map(id),
                value: float("value")?,
                threshold: float("threshold")?,
                localization: get("locid").map(id),
                effect: get("effect").map(id),
                node: get("node").map(id),
                delay: Duration::try_from_secs_f32(float("delay")?).map_err(|error| {
                    BindError::record(record, format!("invalid staff interaction delay: {error}"))
                })?,
                pre_script: script("prescript")?,
                post_script: script("postscript")?,
                show_selection: flag("showselection", true),
                use_parent_rotation: flag("useparentrotation", false),
            })
        })
        .collect()
}

pub(super) fn guest_influence_watches(
    record: &RecordView<'_, '_>,
) -> Result<Vec<GuestInfluenceWatch>, BindError> {
    let mut watches = Vec::<GuestInfluenceWatch>::new();
    let feedback = authored_type_family_elements_named(record, "ZTSharedFeedbackData")
        .into_iter()
        .flat_map(OrderedSourceDocumentNode::element_children);
    for watcher in authored_type_family_elements_named(record, "ZTAISharedInfluenceWatcher") {
        for node in watcher
            .element_children()
            .filter(|node| node.name.eq_ignore_ascii_case("ZTAIInfluenceEffectData"))
        {
            let Some(category) = field(node, "Category").map(id) else {
                continue;
            };
            if watches.iter().any(|watch| watch.category == category) {
                continue;
            }
            let mut sensitivities = Vec::new();
            let mut ranges = Vec::new();
            let feedback_data = field(node, "FeedbackDataName")
                .and_then(|name| {
                    feedback.clone().find(|group| {
                        group.name.eq_ignore_ascii_case("ZTFeedbackData")
                            && field(group, "name")
                                .is_some_and(|key| key.eq_ignore_ascii_case(name))
                    })
                })
                .and_then(|group| {
                    group
                        .element_children()
                        .find(|child| child.name.eq_ignore_ascii_case("data"))
                });
            for child in node.element_children() {
                if child.name.eq_ignore_ascii_case("sensitivity") {
                    for row in child.element_children() {
                        if let Some(value) = read_number(record, row, "value")? {
                            sensitivities.push((id(&row.name), value));
                        }
                    }
                } else if child.name.eq_ignore_ascii_case("range") {
                    let subject = child
                        .element_children()
                        .find(|value| value.name.eq_ignore_ascii_case("BFAISubjectData"));
                    let adjust = |name| {
                        subject
                            .map(|subject| read_number(record, subject, name))
                            .transpose()
                            .map(|value| value.flatten().unwrap_or_default())
                    };
                    let feedback_index = field(child, "FeedbackIndex")
                        .map(str::parse::<usize>)
                        .transpose()
                        .map_err(|error| {
                            BindError::record(
                                record,
                                format!("invalid influence feedback index: {error}"),
                            )
                        })?;
                    let feedback = feedback_index.and_then(|index| {
                        feedback_data?.element_children()
                            .filter(|set| set.name.eq_ignore_ascii_case("ZTFeedbackDataSet"))
                            .nth(index)
                    }).map(|set| {
                        set.element_children()
                            .map(crate::behavior::source::action::feedback_action_lowering::lower_feedback_entry)
                            .collect::<std::io::Result<Vec<_>>>()
                            .map_err(|error| BindError::record(record, error.to_string()))
                    }).transpose()?.unwrap_or_default();
                    ranges.push(GuestInfluenceRange {
                        minimum: read_number(record, child, "Min")?,
                        maximum: read_number(record, child, "Max")?,
                        feedback,
                        happiness_adjustment: adjust("happiness")?,
                        departure_adjustment: adjust("f_DeparturePoints")?,
                    });
                }
            }
            watches.push(GuestInfluenceWatch {
                category,
                update_seconds: f64::from(read_number(record, node, "Update")?.unwrap_or_default()),
                feedback_chance_percent: read_number(record, node, "FeedbackChance")?
                    .unwrap_or_default(),
                sensitivities,
                ranges,
            });
        }
    }
    Ok(watches)
}

fn influence_sources(
    record: &RecordView<'_, '_>,
    node: &OrderedSourceDocumentNode,
) -> Result<Vec<InfluenceSource>, BindError> {
    let radius = read_number(record, node, "defaultRadius")?.unwrap_or_default();
    let group_factor = read_number(record, node, "defaultGroupFactor")?.unwrap_or_default();
    let radius_dropoff = field(node, "defaultRadiusDropoff").map_or_else(|| id("none"), id);
    let group_dropoff = field(node, "defaultGroupDropoff").map_or_else(|| id("none"), id);
    node.element_children()
        .map(|row| {
            Ok(InfluenceSource {
                category: id(&row.name),
                key: field(row, "key").map(id),
                strength: read_number(record, row, "value")?.unwrap_or_default(),
                radius_m: read_number(record, row, "radius")?.unwrap_or(radius),
                radius_dropoff: field(row, "radiusDropoff").map_or(radius_dropoff, id),
                group_factor: read_number(record, row, "groupFactor")?.unwrap_or(group_factor),
                group_dropoff: field(row, "groupDropoff").map_or(group_dropoff, id),
                scaling_attribute: field(row, "scalingAttribute").map(id),
            })
        })
        .collect()
}

pub(super) fn object_influences(
    record: &RecordView<'_, '_>,
) -> Result<Vec<InfluenceSource>, BindError> {
    authored_type_family_elements_named(record, "BFGInfluenceComponent")
        .into_iter()
        .next()
        .map(|node| influence_sources(record, node))
        .transpose()
        .map(Option::unwrap_or_default)
}

pub(super) fn staff_state_variables(
    record: &RecordView<'_, '_>,
) -> Result<Vec<StaffStateVariable>, BindError> {
    let mut needs = BTreeMap::<AssetId, ([f32; 2], f32)>::new();
    for row in authored_type_family_elements_named(record, "BFAIStateVar")
        .into_iter()
        .rev()
    {
        let Some(name) = field(row, "Name") else {
            continue;
        };
        let key = id(name);
        let prior = needs.get(&key).copied();
        let initial = if let Some(value) = read_number(record, row, "Value")? {
            [value; 2]
        } else {
            let low = read_number(record, row, "ValueMin")?
                .unwrap_or_else(|| prior.map_or(0.0, |(range, _)| range[0]));
            [
                low,
                read_number(record, row, "ValueMax")?
                    .unwrap_or_else(|| prior.map_or(low, |(range, _)| range[1])),
            ]
        };
        if initial[1] < initial[0] {
            return Err(BindError::record(
                record,
                "staff need initial range is reversed",
            ));
        }
        let threshold = read_number(record, row, "TriggerThreshold")?
            .unwrap_or_else(|| prior.map_or(0.0, |(_, threshold)| threshold));
        needs.insert(key, (initial, threshold));
    }
    let mut deltas = BTreeMap::new();
    for row in authored_type_family_elements_named(record, "BFAINeedAdjusts")
        .into_iter()
        .rev()
        .filter(|row| field(row, "Name").is_none())
    {
        for (name, value) in row
            .attributes()
            .filter(|(name, value)| !name.eq_ignore_ascii_case("Name") && !value.trim().is_empty())
        {
            deltas.insert(id(name), number(record, name, value)?);
        }
    }
    Ok(needs
        .into_iter()
        .map(|(name, (initial, trigger_threshold))| StaffStateVariable {
            name,
            value: initial[0],
            trigger_threshold,
            initial_range: (initial[0] != initial[1]).then_some(initial),
            adjustment: deltas.get(&name).copied(),
        })
        .collect())
}

pub(super) fn world_guest_influence_policy(
    record: &RecordView<'_, '_>,
) -> Result<Option<GuestInfluencePolicy>, BindError> {
    let node = record.source_document_element();
    if !node.name.eq_ignore_ascii_case("BFGInfluenceMgr") || field(node, "configData").is_some() {
        return Ok(None);
    }
    let Some(cells) = field(node, "constantWork") else {
        return Ok(None);
    };
    let sources = node
        .element_children()
        .find(|child| child.name.eq_ignore_ascii_case("terrain"))
        .map(|terrain| influence_sources(record, terrain))
        .transpose()?
        .unwrap_or_default();
    Ok(Some(GuestInfluencePolicy {
        cells_per_update: cells.parse().map_err(|error| {
            BindError::record(record, format!("invalid influence work count: {error}"))
        })?,
        update_delay_seconds: read_number(record, node, "updateDelay")?.unwrap_or_default(),
        native_factor: read_number(record, node, "nativeFactor")?.unwrap_or(1.0),
        biome_factor: read_number(record, node, "biomeFactor")?.unwrap_or(1.0),
        location_factor: read_number(record, node, "locationFactor")?.unwrap_or(1.0),
        sources,
    }))
}
