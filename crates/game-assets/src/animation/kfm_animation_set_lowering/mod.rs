use std::{collections::BTreeMap, io, path::Path, time::Duration};

use openzt2_game_data::animation::animation_set::{
    AuthoredAnimationGraphEdge, AuthoredAnimationGraphMetadata, AuthoredAnimationGraphNode,
    AuthoredAnimationPlaybackPolicy, AuthoredAnimationSetClipReference,
    AuthoredAnimationSetDocument, AuthoredKfmTransition, KfmTransitionKind,
};

use crate::model_source::native_animation_source_lowering::kf_sequence_label;

struct KfmAnimationDeclaration {
    authored_event_code: u32,
    animation_clip_asset_key: String,
    animation_clip_asset_path: String,
    authored_transitions: Vec<KfmAnimationTransitionDeclaration>,
}

struct KfmAnimationTransitionDeclaration {
    target_authored_event_code: u32,
    kind: KfmTransitionKind,
    duration: Duration,
    intermediate_event_codes: Vec<u32>,
    text_key_pairs: Vec<[Option<String>; 2]>,
}

pub(super) fn lower_kfm_source_to_animation_set_document(
    animation_set_asset_path: &str,
    kfm_source_bytes: &[u8],
) -> io::Result<AuthoredAnimationSetDocument> {
    let kfm_source = std::str::from_utf8(kfm_source_bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let animation_set_parent_path = Path::new(animation_set_asset_path).parent();
    let mut model_path_prefix = "";
    let mut clip_path_prefix = "";
    let mut model_asset_path = None;
    let mut kfm_animation_declarations = Vec::<KfmAnimationDeclaration>::new();

    for kfm_record in kfm_source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(';'))
    {
        let kfm_record_fields = kfm_record
            .trim_end_matches('#')
            .split('#')
            .collect::<Vec<_>>();
        match kfm_record_fields.first().copied() {
            Some("DEFAULTPATHS")
                if matches!(kfm_record_fields.get(1).copied(), Some("0" | "1")) => {}
            Some("DEFAULTNIFPATH" | "DEFAULTKFPATH") => {
                let prefix = kfm_record_fields
                    .get(1)
                    .copied()
                    .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, kfm_record))?;
                let prefix = if prefix == "NULL" { "" } else { prefix };
                if kfm_record_fields[0] == "DEFAULTNIFPATH" {
                    model_path_prefix = prefix;
                } else {
                    clip_path_prefix = prefix;
                }
            }
            Some("END_KFM_FILE") => break,
            Some("MODEL") => {
                model_asset_path = kfm_record_fields.get(1).map(|source_path| {
                    resolve_kfm_source_reference(
                        animation_set_parent_path,
                        &format!("{model_path_prefix}{source_path}"),
                        None,
                    )
                });
            }
            Some("ANIMATION") => {
                let authored_event_code = kfm_record_fields
                    .get(1)
                    .and_then(|field| field.strip_prefix("EVENTCODE "))
                    .and_then(|field| field.parse::<u32>().ok())
                    .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, kfm_record))?;
                let animation_clip_asset_key = kfm_record_fields
                    .get(2)
                    .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, kfm_record))?;
                let animation_clip_source_path = kfm_record_fields
                    .get(3)
                    .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, kfm_record))?;
                let sequence_index = kfm_record_fields
                    .get(4)
                    .and_then(|field| field.strip_prefix("INDEX "))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, kfm_record))?;
                let mut animation_clip_asset_path = resolve_kfm_source_reference(
                    animation_set_parent_path,
                    &format!("{clip_path_prefix}{animation_clip_source_path}"),
                    Some("kf"),
                );
                animation_clip_asset_path.push('#');
                animation_clip_asset_path.push_str(&kf_sequence_label(sequence_index));
                kfm_animation_declarations.push(KfmAnimationDeclaration {
                    authored_event_code,
                    animation_clip_asset_key: (*animation_clip_asset_key).to_owned(),
                    animation_clip_asset_path,
                    authored_transitions: Vec::new(),
                });
            }
            Some("TRANSITION") => {
                let transition = parse_kfm_transition(&kfm_record_fields[1..])
                    .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, kfm_record))?;
                kfm_animation_declarations
                    .last_mut()
                    .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, kfm_record))?
                    .authored_transitions
                    .push(transition);
            }
            Some(record_kind) => {
                return Err(invalid_kfm_record(
                    animation_set_asset_path,
                    &format!("unsupported {record_kind} record: {kfm_record}"),
                ));
            }
            None => return Err(invalid_kfm_record(animation_set_asset_path, kfm_record)),
        }
    }

    let model_asset_path = model_asset_path
        .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, "missing MODEL"))?;
    let animation_graph_nodes = kfm_animation_declarations
        .iter()
        .map(|kfm_animation_declaration| AuthoredAnimationGraphNode {
            animation_graph_node_asset_key: kfm_animation_declaration
                .animation_clip_asset_key
                .clone(),
            animation_clip_asset_keys: vec![kfm_animation_declaration
                .animation_clip_asset_key
                .clone()],
            authored_animation_attributes: Vec::new(),
        })
        .collect();
    let animation_graph_order = kfm_animation_declarations
        .iter()
        .map(|kfm_animation_declaration| kfm_animation_declaration.animation_clip_asset_key.clone())
        .collect();
    let mut keys_by_event_code = BTreeMap::new();
    for animation in &kfm_animation_declarations {
        keys_by_event_code.insert(
            animation.authored_event_code,
            &animation.animation_clip_asset_key,
        );
    }
    let key_for_event_code = |event_code| {
        keys_by_event_code
            .get(&event_code)
            .map(|key| (*key).clone())
            .ok_or_else(|| {
                invalid_kfm_record(
                    animation_set_asset_path,
                    &format!("missing animation event code {event_code}"),
                )
            })
    };
    let mut animation_graph_edges = Vec::new();
    for animation in &kfm_animation_declarations {
        for transition in &animation.authored_transitions {
            let transition_animation_clip_asset_keys = transition
                .intermediate_event_codes
                .iter()
                .map(|&code| key_for_event_code(code))
                .collect::<io::Result<Vec<_>>>()?;
            let kind = if transition_animation_clip_asset_keys.is_empty() {
                transition.kind
            } else {
                KfmTransitionKind::Chain
            };
            animation_graph_edges.push(AuthoredAnimationGraphEdge {
                source_animation_graph_node_asset_key: animation.animation_clip_asset_key.clone(),
                target_animation_graph_node_asset_key: key_for_event_code(
                    transition.target_authored_event_code,
                )?,
                transition_animation_clip_asset_keys,
                authored_animation_attributes: Vec::new(),
                kfm_transition: Some(AuthoredKfmTransition {
                    kind,
                    duration: transition.duration,
                    text_key_pairs: transition.text_key_pairs.clone(),
                }),
            });
        }
    }

    Ok(AuthoredAnimationSetDocument {
        model_asset_path: model_asset_path.clone(),
        skeleton_asset_path: model_asset_path,
        animation_clips: kfm_animation_declarations
            .iter()
            .map(
                |kfm_animation_declaration| AuthoredAnimationSetClipReference {
                    animation_clip_asset_key: kfm_animation_declaration
                        .animation_clip_asset_key
                        .clone(),
                    animation_clip_asset_path: kfm_animation_declaration
                        .animation_clip_asset_path
                        .clone(),
                    playback_policy: AuthoredAnimationPlaybackPolicy::default(),
                },
            )
            .collect(),
        animation_graph_nodes,
        animation_graph_order,
        animation_graph_edges,
        animation_graph_metadata: Some(AuthoredAnimationGraphMetadata {
            authored_graph_name: None,
            authored_graph_version: Some(1),
            authored_animation_attributes: Vec::new(),
        }),
    })
}

fn parse_kfm_transition(fields: &[&str]) -> Option<KfmAnimationTransitionDeclaration> {
    let [target, kind, duration, chain_count, remaining @ ..] = fields else {
        return None;
    };
    let target_authored_event_code = target.strip_prefix("EVENTCODE ")?.trim().parse().ok()?;
    let kind = match kind.strip_prefix("TYPE ")?.trim() {
        "BLEND" => KfmTransitionKind::Blend,
        "MORPH" => KfmTransitionKind::Morph,
        _ => return None,
    };
    let seconds = duration
        .strip_prefix("DURATION ")?
        .trim()
        .parse::<f64>()
        .ok()?;
    let duration = Duration::try_from_secs_f64(seconds).ok()?;
    let chain_count = chain_count
        .strip_prefix("INTERMEDIATEANIMS ")?
        .trim()
        .parse()
        .ok()?;
    let (intermediate, remaining) = remaining.split_at_checked(chain_count)?;
    let intermediate_event_codes = intermediate
        .iter()
        .map(|value| value.trim().parse().ok())
        .collect::<Option<Vec<_>>>()?;
    let (pairs_count, pairs) = remaining.split_first()?;
    let pairs_count = pairs_count
        .strip_prefix("TEXTKEYPAIRS ")?
        .trim()
        .parse::<usize>()
        .ok()?;
    if pairs_count.checked_mul(2)? != pairs.len() {
        return None;
    }
    let text_key_pairs = pairs
        .chunks_exact(2)
        .map(|pair| [pair[0], pair[1]].map(|key| (key != "NULL").then(|| key.to_owned())))
        .filter(|pair| pair.iter().any(Option::is_some))
        .collect();
    Some(KfmAnimationTransitionDeclaration {
        target_authored_event_code,
        kind,
        duration,
        intermediate_event_codes,
        text_key_pairs,
    })
}

fn resolve_kfm_source_reference(
    animation_set_parent_path: Option<&Path>,
    authored_source_path: &str,
    required_extension: Option<&str>,
) -> String {
    let slash_normalized_source_path = authored_source_path.replace('\\', "/");
    let relative_source_path = if slash_normalized_source_path.as_bytes().get(1) == Some(&b':') {
        slash_normalized_source_path
            .rsplit('/')
            .next()
            .unwrap_or(&slash_normalized_source_path)
            .into()
    } else {
        std::path::PathBuf::from(slash_normalized_source_path)
    };
    let resolved_source_path = animation_set_parent_path
        .map_or(relative_source_path.clone(), |parent_path| {
            parent_path.join(&relative_source_path)
        });

    required_extension
        .map_or(resolved_source_path.clone(), |extension| {
            resolved_source_path.with_extension(extension)
        })
        .to_string_lossy()
        .replace('\\', "/")
}

fn invalid_kfm_record(animation_set_asset_path: &str, kfm_record: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid KFM {animation_set_asset_path}: {kfm_record}"),
    )
}
