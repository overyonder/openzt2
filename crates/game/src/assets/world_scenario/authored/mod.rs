//! Direct adapter from shipped Blue Fang document shapes to the owned scenario schema input.

use crate::assets::world_scenario::world_scenario_source_reference_resolution::{
    SourceReference, SourceReferenceKind,
};

use std::{collections::BTreeMap, io};

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode,
};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::{
    world_definitions::simulation_time::SimulationTimingDefinition,
    world_scenario::{
        CampaignScenarioRecord, PhotoChallengeMemberRecord, PhotoChallengeRule,
        PhotoChallengeScriptBinding, PhotoChallengeSetProgression, PhotoChallengeSetRecord,
        PhotoChallengeSetSelection, ScenarioCampaignRecord, ScenarioDefinitionRecord,
        ScenarioObjectiveRecord, ScenarioRecordFlags, ScenarioScriptBinding, ScenarioScriptPhase,
        WorldLocationRecord, WorldMapRecord, WorldMapSupportedGameModeFlags, WorldScenarioDocument,
    },
    AssetId,
};

pub(in crate::assets::world_scenario) fn lower_documents(
    documents: &[&OrderedSourceDocument],
    dependencies: &[SourceReference],
    actor_scene_paths: &BTreeMap<String, String>,
    timing: Option<&SimulationTimingDefinition>,
) -> io::Result<WorldScenarioDocument> {
    let mut output = WorldScenarioDocument::default();
    let mut bindings = Vec::new();
    let mut photo_bindings = Vec::new();
    for document in documents {
        let root = local(&document.root.name);
        let path = document.path.key();
        let lower_path = path.to_ascii_lowercase();
        let lowered: io::Result<()> = match root {
            "BFContext" | "saveRoot" if lower_path.ends_with(".zt2") => {
                let timing = timing.ok_or_else(|| {
                    at_root(document, "starting zoo has no simulation timing dependency")
                })?;
                output.starting_zoos.push(super::starting_zoo::lower(
                    document,
                    dependencies,
                    actor_scene_paths,
                    timing,
                )?);
                Ok(())
            }
            "maps" if original_map_index(document) => {
                output.maps.extend(lower_maps(document, dependencies)?);
                Ok(())
            }
            "BFCampaign" => {
                output.campaigns.push(lower_campaign(document)?);
                Ok(())
            }
            "entries" if path.to_ascii_lowercase().contains("locations/") => {
                output.locations.extend(lower_locations(document)?);
                Ok(())
            }
            "Campaign" if path.eq_ignore_ascii_case("scenario/campaign/index.xml") => {
                output.campaign_order = document
                    .root
                    .element_children()
                    .filter(|node| local(&node.name) == "campaign")
                    .map(|node| {
                        node.first_text()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .and_then(|value| std::path::Path::new(value).file_stem())
                            .and_then(|value| value.to_str())
                            .map(AssetId::from_key)
                            .ok_or_else(|| at(document, node, "campaign entry has no UTF-8 stem"))
                    })
                    .collect::<io::Result<_>>()?;
                Ok(())
            }
            "challenges" if lower_path.contains("photochall/") => {
                lower_photo_challenges(document, &mut output, &mut photo_bindings)?;
                Ok(())
            }
            _ => {
                let mut rules = Vec::new();
                let roots = document.root.element_children().collect::<Vec<_>>();
                collect_rules(
                    &roots,
                    None,
                    document.root.attribute("chain").is_some_and(truthy),
                    &mut rules,
                );
                if !rules.is_empty() {
                    let scenario = source_stem(&path)?;
                    let mut objectives = Vec::with_capacity(rules.len());
                    for (index, rule) in rules.into_iter().enumerate() {
                        let objective = format!("{scenario}:rule:{index}");
                        let prerequisite = rule
                            .prerequisite
                            .as_deref()
                            .and_then(|value| value.strip_prefix("rule-source-"))
                            .map(|value| format!("{scenario}:rule:{value}"));
                        collect_bindings(
                            document,
                            rule.node,
                            &scenario,
                            &objective,
                            "evaluate",
                            ScenarioScriptPhase::Evaluate,
                            &mut bindings,
                        )?;
                        collect_bindings(
                            document,
                            rule.node,
                            &scenario,
                            &objective,
                            "success",
                            ScenarioScriptPhase::Success,
                            &mut bindings,
                        )?;
                        collect_bindings(
                            document,
                            rule.node,
                            &scenario,
                            &objective,
                            "failure",
                            ScenarioScriptPhase::Failure,
                            &mut bindings,
                        )?;
                        let id = AssetId::from_key(&objective);
                        objectives.push(ScenarioObjectiveRecord {
                            id,
                            category: child(rule.node, "info")
                                .and_then(|info| info.attribute("type"))
                                .map(AssetId::from_key)
                                .unwrap_or_default(),
                            text_key: status_text(rule.node, "neutral")
                                .as_deref()
                                .map(AssetId::from_key)
                                .unwrap_or_default(),
                            success_text_key: status_text(rule.node, "success")
                                .as_deref()
                                .map(AssetId::from_key)
                                .unwrap_or_default(),
                            failure_text_key: status_text(rule.node, "failure")
                                .as_deref()
                                .map(AssetId::from_key)
                                .unwrap_or_default(),
                            prerequisite_objectives: prerequisite
                                .as_deref()
                                .map(|value| vec![AssetId::from_key(value)])
                                .unwrap_or_default(),
                            hidden: rule.node.attribute("hidden").is_some_and(truthy),
                        });
                    }
                    let lower_path = path.to_ascii_lowercase();
                    let mut flags = ScenarioRecordFlags::ALLOW_PAUSE;
                    if lower_path.contains("tutorial") {
                        flags = flags.with_additional_flags(ScenarioRecordFlags::TUTORIAL);
                    }
                    if lower_path.contains("challenge") {
                        flags = flags.with_additional_flags(ScenarioRecordFlags::CHALLENGE);
                    }
                    let record = ScenarioDefinitionRecord {
                        id: AssetId::from_key(&scenario),
                        name_key: document
                            .root
                            .attribute("name")
                            .map(AssetId::from_key)
                            .unwrap_or_default(),
                        description_key: document
                            .root
                            .attribute("description")
                            .map(AssetId::from_key)
                            .unwrap_or_default(),
                        objectives,
                        time_limit_ticks: 0,
                        flags,
                    };
                    if let Some(existing) = output
                        .scenarios
                        .iter_mut()
                        .find(|candidate| candidate.id == record.id)
                    {
                        existing.objectives = record.objectives;
                        existing.flags = record.flags;
                        if existing.name_key == AssetId::default() {
                            existing.name_key = record.name_key;
                        }
                        if existing.description_key == AssetId::default() {
                            existing.description_key = record.description_key;
                        }
                    } else {
                        output.scenarios.push(record);
                    }
                    Ok(())
                } else {
                    Ok(())
                }
            }
        };
        lowered?;
    }
    let challenge_bindings = bindings
        .iter()
        .filter_map(|binding| {
            if binding.phase != ScenarioScriptPhase::Evaluate {
                return None;
            }
            documents
                .iter()
                .find_map(|document| {
                    document
                        .path
                        .key()
                        .to_ascii_lowercase()
                        .contains("scenario/goals/challenge/")
                        .then(|| source_stem(&document.path.key()).ok())
                        .flatten()
                        .filter(|scenario| AssetId::from_key(scenario) == binding.scenario)
                })
                .map(|scenario| (scenario, binding.script.clone()))
        })
        .fold(
            BTreeMap::<String, String>::new(),
            |mut scripts, (scenario, script)| {
                scripts.entry(scenario).or_insert(script);
                scripts
            },
        );
    bindings.extend(challenge_bindings.into_iter().map(|(scenario, script)| {
        ScenarioScriptBinding {
            scenario: AssetId::from_key(&scenario),
            objective: AssetId::from_key(&format!("{scenario}:validate")),
            script,
            entry: "validate".to_owned(),
            phase: ScenarioScriptPhase::Validate,
        }
    }));
    output.scenario_script_bindings = bindings;
    output.photo_challenge_script_bindings = photo_bindings;
    Ok(output)
}

fn lower_photo_challenges(
    document: &OrderedSourceDocument,
    output: &mut WorldScenarioDocument,
    bindings: &mut Vec<PhotoChallengeScriptBinding>,
) -> io::Result<()> {
    for node in document.root.element_children() {
        match local(&node.name) {
            "ZTPhotoChallenge" => {
                let name = required(document, node, "name")?;
                let id = AssetId::from_key(name);
                let script = required(document, node, "script")?.to_owned();
                let entry = required(document, node, "function")?.to_owned();
                output.photo_challenges.push(PhotoChallengeRule {
                    id,
                    instruction_key: AssetId::from_key(required(document, node, "stt")?),
                    completion_key: AssetId::from_key(
                        node.attribute("cstt")
                            .or_else(|| node.attribute("csst"))
                            .ok_or_else(|| at(document, node, "missing cstt/csst"))?,
                    ),
                    difficulty: required(document, node, "difficulty")?
                        .parse()
                        .map_err(|_| {
                            at(
                                document,
                                node,
                                "photo challenge difficulty is not an unsigned byte",
                            )
                        })?,
                });
                bindings.push(PhotoChallengeScriptBinding {
                    challenge: AssetId::from_key(name),
                    script,
                    entry,
                });
            }
            "ZTPhotoChallengeSet" => {
                let name = required(document, node, "name")?;
                let members = node
                    .element_children()
                    .enumerate()
                    .map(|(ordinal, member)| {
                        Ok(PhotoChallengeMemberRecord {
                            challenge: AssetId::from_key(local(&member.name)),
                            ordinal: u16::try_from(ordinal).map_err(|_| {
                                at(document, member, "photo challenge set exceeds u16")
                            })?,
                        })
                    })
                    .collect::<io::Result<Vec<_>>>()?;
                output.photo_challenge_sets.push(PhotoChallengeSetRecord {
                    id: AssetId::from_key(name),
                    title_key: AssetId::from_key(required(document, node, "title")?),
                    members,
                    selection: PhotoChallengeSetSelection::AllMembersInAuthoredOrder,
                    progression: PhotoChallengeSetProgression::CompleteEachMemberOnce,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

struct RuleEntry<'a> {
    node: &'a OrderedSourceDocumentNode,
    prerequisite: Option<String>,
}
fn collect_rules<'a>(
    nodes: &[&'a OrderedSourceDocumentNode],
    inherited: Option<String>,
    chain: bool,
    output: &mut Vec<RuleEntry<'a>>,
) -> Option<String> {
    let mut previous = inherited;
    for node in nodes {
        match local(&node.name) {
            "BFScenarioRule" => {
                let id = format!("rule-source-{}", output.len());
                output.push(RuleEntry {
                    node,
                    prerequisite: chain.then(|| previous.clone()).flatten(),
                });
                previous = Some(id);
            }
            "BFScenarioGroup" | "ZTScenarioMgr" | "children" => {
                let children = if local(&node.name) == "children" {
                    node.element_children().collect::<Vec<_>>()
                } else {
                    child(node, "children")
                        .map(|value| value.element_children().collect())
                        .unwrap_or_default()
                };
                let nested = collect_rules(
                    &children,
                    previous.clone(),
                    node.attribute("chain").is_some_and(truthy),
                    output,
                );
                if chain {
                    previous = nested.or(previous);
                }
            }
            _ => {}
        }
    }
    previous
}

fn collect_bindings(
    document: &OrderedSourceDocument,
    rule: &OrderedSourceDocumentNode,
    scenario: &str,
    objective: &str,
    phase_name: &str,
    phase: ScenarioScriptPhase,
    output: &mut Vec<ScenarioScriptBinding>,
) -> io::Result<()> {
    let Some(container) = rule
        .element_children()
        .find(|node| local(&node.name).eq_ignore_ascii_case(phase_name))
    else {
        return Ok(());
    };
    for action in descendants(container.element_children())
        .filter(|node| local(&node.name) == "BFScenarioScriptAction")
    {
        let entry = required(document, action, "entry")?.trim_start_matches(['@', '#']);
        output.push(ScenarioScriptBinding {
            scenario: AssetId::from_key(scenario),
            objective: AssetId::from_key(objective),
            script: required(document, action, "script")?.to_owned(),
            entry: entry.strip_suffix("()").unwrap_or(entry).to_owned(),
            phase,
        });
    }
    Ok(())
}

fn child<'a>(
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    node.element_children()
        .find(|child| local(&child.name) == name)
}
fn status_text(node: &OrderedSourceDocumentNode, status: &str) -> Option<String> {
    child(node, "info")
        .and_then(|info| child(info, status))
        .and_then(|value| {
            value
                .attribute("locid")
                .or_else(|| value.attribute("overview"))
        })
        .map(str::to_owned)
}
fn truthy(value: &str) -> bool {
    value == "1" || value.eq_ignore_ascii_case("true")
}

fn source_stem(path: &str) -> io::Result<String> {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("{path}: scenario path has no UTF-8 stem")))
}

fn lower_locations(document: &OrderedSourceDocument) -> io::Result<Vec<WorldLocationRecord>> {
    descendants(document.root.element_children())
        .filter(|node| local(&node.name) == "ZTLocationEntry")
        .filter_map(|entry| entry.attribute("name").map(|name| (entry, name)))
        .filter(|(entry, _)| {
            entry
                .element_children()
                .any(|node| local(&node.name) == "coordinate")
        })
        .map(|(entry, name)| {
            let coordinate_node = entry
                .element_children()
                .find(|node| local(&node.name) == "coordinate")
                .expect("filtered location has a coordinate");
            let longitude = coordinate(document, coordinate_node, "long", 'E', 'W')?;
            let latitude = coordinate(document, coordinate_node, "lat", 'N', 'S')?;
            if !(-180.0..=180.0).contains(&longitude) || !(-90.0..=90.0).contains(&latitude) {
                return Err(at(
                    document,
                    coordinate_node,
                    "globe coordinate is outside its valid range",
                ));
            }
            Ok(WorldLocationRecord {
                id: AssetId::from_key(name),
                name_key: AssetId::from_key(required(document, entry, "locid")?),
                globe_marker: [centidegrees(longitude)?, centidegrees(latitude)?],
            })
        })
        .collect()
}

fn coordinate(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    name: &str,
    positive: char,
    negative: char,
) -> io::Result<f32> {
    let value = required(document, node, name)?;
    let value = value.trim();
    if let Ok(coordinate) = value.parse::<f32>() {
        return Ok(coordinate);
    }
    let suffix_start = value
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index);
    let (number, sign) = value.split_at(suffix_start);
    let magnitude = number.trim().parse::<f32>().map_err(|_| {
        at(
            document,
            node,
            format!("{name} is not a coordinate: {value}"),
        )
    })?;
    match sign.chars().next() {
        Some(direction) if direction.eq_ignore_ascii_case(&positive) => Ok(magnitude),
        Some(direction) if direction.eq_ignore_ascii_case(&negative) => Ok(-magnitude),
        _ => Err(at(
            document,
            node,
            format!("{name} has unknown direction: {value}"),
        )),
    }
}

fn lower_maps(
    document: &OrderedSourceDocument,
    dependencies: &[SourceReference],
) -> io::Result<Vec<WorldMapRecord>> {
    let scenario = document
        .path
        .key()
        .to_ascii_lowercase()
        .contains("maps/scenario/");
    let modes = if scenario {
        "campaign"
    } else {
        "freeform|challenge"
    };
    document
        .root
        .element_children()
        .enumerate()
        .filter(|(_, wrapper)| {
            let id = local(&wrapper.name);
            find_reference(dependencies, id, SourceReferenceKind::Terrain).is_some()
                && find_reference(dependencies, id, SourceReferenceKind::Texture).is_some()
        })
        .map(|(order, wrapper)| {
            let mut children = wrapper.element_children();
            let data = children
                .next()
                .ok_or_else(|| at(document, wrapper, "map wrapper has no ZTMapData"))?;
            if local(&data.name) != "ZTMapData" || children.next().is_some() {
                return Err(at(
                    document,
                    wrapper,
                    "map wrapper must contain exactly one ZTMapData",
                ));
            }
            let id = local(&wrapper.name);
            let terrain = find_reference(dependencies, id, SourceReferenceKind::Terrain)
                .expect("filtered map has a terrain asset");
            let thumbnail = find_reference(dependencies, id, SourceReferenceKind::Texture)
                .expect("filtered map has a thumbnail asset");
            let name = required(document, data, "name")?;
            let size = required(document, data, "size")?;
            let biome = required(document, data, "biome")?.to_ascii_lowercase();
            let location_name = required(document, data, "location")?;
            let expansion_pack_filter_identifier = data
                .attribute("xpack")
                .unwrap_or("0")
                .parse::<u16>()
                .map_err(|_| {
                    at(
                        document,
                        data,
                        "map xpack must be an unsigned 16-bit integer",
                    )
                })?;
            let environment_biome = if biome == "tropicalrainforest" {
                "rainforest"
            } else {
                &biome
            };
            let suffix = match size.to_ascii_lowercase().as_str() {
                "mapsize:large" => "",
                "mapsize:medium" => "_medium",
                "mapsize:small" => "_small",
                _ => return Err(at(document, data, format!("unknown map size {size}"))),
            };
            let environment = format!("world/environments/env1{environment_biome}{suffix}.xml");
            Ok(WorldMapRecord {
                id: AssetId::from_key(id),
                catalogue_order: u32::try_from(order)
                    .map_err(|_| at(document, data, "map order exceeds u32"))?,
                name_key: AssetId::from_key(name),
                location: AssetId::from_key(location_name),
                biome: AssetId::from_key(&biome),
                biome_key: AssetId::from_key(&format!("{biome}:icon_stt")),
                size_key: AssetId::from_key(size),
                description_key: data
                    .attribute("protected")
                    .filter(|value| !value.is_empty())
                    .map(AssetId::from_key),
                thumbnail: thumbnail.id,
                environment: AssetId::from_virtual_path(&environment),
                camera: AssetId::from_virtual_path("world/cameras/overheadcam.xml"),
                terrain: terrain.id,
                starting_zoo: AssetId::from_key(&format!("start:{id}")),
                expansion_pack_filter_identifier,
                supported_game_modes: if modes == "campaign" {
                    WorldMapSupportedGameModeFlags::CAMPAIGN
                } else {
                    WorldMapSupportedGameModeFlags::FREEFORM
                        .with_additional_flags(WorldMapSupportedGameModeFlags::CHALLENGE)
                },
            })
        })
        .collect()
}

fn find_reference<'a>(
    dependencies: &'a [SourceReference],
    id: &str,
    kind: SourceReferenceKind,
) -> Option<&'a SourceReference> {
    dependencies
        .iter()
        .filter(|asset| asset.kind == kind)
        .find(|asset| {
            std::path::Path::new(&asset.path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case(id)
                        || name
                            .to_ascii_lowercase()
                            .starts_with(&format!("{}.", id.to_ascii_lowercase()))
                })
        })
}

fn lower_campaign(document: &OrderedSourceDocument) -> io::Result<ScenarioCampaignRecord> {
    let path = document.path.key();
    let id = path
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.').map(|pair| pair.0))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(format!("{path}: campaign path has no stem")))?;
    let name = document
        .root
        .attribute("name")
        .ok_or_else(|| invalid(format!("{path}: campaign has no name")))?;
    let entries = document
        .root
        .element_children()
        .filter(|node| local(&node.name) == "entry")
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return Err(invalid(format!("{path}: campaign has no entries")));
    }
    let description = entries[0].attribute("description").unwrap_or(name);
    let scenarios =
        entries
            .iter()
            .map(|entry| {
                let id = required(document, entry, "key")?;
                let map = required(document, entry, "map")?;
                let cash = required(document, entry, "cash")?
                    .parse::<i64>()
                    .ok()
                    .and_then(|dollars| dollars.checked_mul(100))
                    .ok_or_else(|| at(document, entry, "campaign cash is not whole dollars"))?;
                Ok(CampaignScenarioRecord {
                    id: AssetId::from_key(id),
                    map: AssetId::from_key(map),
                    starting_zoo: AssetId::from_key(&format!("start:{map}")),
                    starting_cash_cents: cash,
                    difficulty: AssetId::from_key(&canonicalize_source_document_record_key(
                        required(document, entry, "difficulty")?,
                    )),
                    name_key: AssetId::from_key(&canonicalize_source_document_record_key(
                        required(document, entry, "name")?,
                    )),
                    description_key: AssetId::from_key(&canonicalize_source_document_record_key(
                        entry.attribute("description").unwrap_or(id),
                    )),
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
    Ok(ScenarioCampaignRecord {
        id: AssetId::from_key(id),
        name_key: AssetId::from_key(&canonicalize_source_document_record_key(name)),
        description_key: AssetId::from_key(&canonicalize_source_document_record_key(description)),
        scenarios,
    })
}

fn original_map_index(document: &OrderedSourceDocument) -> bool {
    let mut entries = document.root.element_children().peekable();
    entries.peek().is_some()
        && entries.all(|entry| {
            let mut children = entry.element_children();
            children
                .next()
                .is_some_and(|node| local(&node.name) == "ZTMapData")
                && children.next().is_none()
        })
}

fn descendants<'a>(
    roots: impl Iterator<Item = &'a OrderedSourceDocumentNode>,
) -> impl Iterator<Item = &'a OrderedSourceDocumentNode> {
    let mut stack = roots.collect::<Vec<_>>();
    stack.reverse();
    std::iter::from_fn(move || {
        let node = stack.pop()?;
        let mut children = node.element_children().collect::<Vec<_>>();
        children.reverse();
        stack.extend(children);
        Some(node)
    })
}

fn required<'a>(
    document: &OrderedSourceDocument,
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> io::Result<&'a str> {
    node.attribute(name)
        .ok_or_else(|| at(document, node, format!("missing {name}")))
}

fn at(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    message: impl std::fmt::Display,
) -> io::Error {
    invalid(format!(
        "{}:{}..{}: {message}",
        document.path.key(),
        node.span.start,
        node.span.end
    ))
}

fn at_root(document: &OrderedSourceDocument, message: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "{}:{}..{}: {message}",
            document.path.key(),
            document.root.span.start,
            document.root.span.end
        ),
    )
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}
fn centidegrees(value: f32) -> io::Result<i16> {
    let scaled = (value * 100.0).round();
    if scaled.is_finite() && scaled >= f32::from(i16::MIN) && scaled <= f32::from(i16::MAX) {
        Ok(scaled as i16)
    } else {
        Err(invalid(
            "coordinate cannot be represented in signed centidegrees",
        ))
    }
}
