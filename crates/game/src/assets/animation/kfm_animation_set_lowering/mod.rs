use std::{io, path::Path};

use openzt2_game_data::animation::animation_set::{
    AuthoredAnimationAttribute, AuthoredAnimationGraphEdge, AuthoredAnimationGraphMetadata,
    AuthoredAnimationGraphNode, AuthoredAnimationPlaybackPolicy, AuthoredAnimationSetClipReference,
    AuthoredAnimationSetDocument,
};

struct KfmAnimationDeclaration {
    authored_event_code: u32,
    animation_clip_asset_key: String,
    animation_clip_asset_path: String,
    authored_transitions: Vec<KfmAnimationTransitionDeclaration>,
}

struct KfmAnimationTransitionDeclaration {
    target_authored_event_code: u32,
    authored_blend_duration: String,
}

pub(super) fn lower_kfm_source_to_animation_set_document(
    animation_set_asset_path: &str,
    kfm_source_bytes: &[u8],
) -> io::Result<AuthoredAnimationSetDocument> {
    let kfm_source = std::str::from_utf8(kfm_source_bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let animation_set_parent_path = Path::new(animation_set_asset_path).parent();
    let mut model_asset_path = None;
    let mut kfm_animation_declarations = Vec::<KfmAnimationDeclaration>::new();

    for kfm_record in kfm_source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(';'))
    {
        let kfm_record_fields = kfm_record.split('#').collect::<Vec<_>>();
        match kfm_record_fields.first().copied() {
            Some("DEFAULTPATHS") if kfm_record_fields.get(1) == Some(&"1") => {}
            Some("DEFAULTNIFPATH" | "DEFAULTKFPATH")
                if kfm_record_fields.get(1) == Some(&"NULL") => {}
            Some("END_KFM_FILE") => break,
            Some("MODEL") => {
                model_asset_path = kfm_record_fields.get(1).map(|source_path| {
                    resolve_kfm_source_reference(animation_set_parent_path, source_path, None)
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
                kfm_animation_declarations.push(KfmAnimationDeclaration {
                    authored_event_code,
                    animation_clip_asset_key: (*animation_clip_asset_key).to_owned(),
                    animation_clip_asset_path: resolve_kfm_source_reference(
                        animation_set_parent_path,
                        animation_clip_source_path,
                        Some("kf"),
                    ),
                    authored_transitions: Vec::new(),
                });
            }
            Some("TRANSITION") => {
                let target_authored_event_code = kfm_record_fields
                    .get(1)
                    .and_then(|field| field.strip_prefix("EVENTCODE "))
                    .and_then(|field| field.parse::<u32>().ok())
                    .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, kfm_record))?;
                let authored_blend_duration = kfm_record_fields
                    .iter()
                    .find_map(|field| field.strip_prefix("DURATION "))
                    .unwrap_or("0");
                kfm_animation_declarations
                    .last_mut()
                    .ok_or_else(|| invalid_kfm_record(animation_set_asset_path, kfm_record))?
                    .authored_transitions
                    .push(KfmAnimationTransitionDeclaration {
                        target_authored_event_code,
                        authored_blend_duration: authored_blend_duration.to_owned(),
                    });
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
    let animation_graph_edges = kfm_animation_declarations
        .iter()
        .flat_map(|kfm_animation_declaration| {
            kfm_animation_declaration
                .authored_transitions
                .iter()
                .map(|authored_transition| {
                    kfm_animation_declarations
                        .iter()
                        .find(|candidate_kfm_animation_declaration| {
                            candidate_kfm_animation_declaration.authored_event_code
                                == authored_transition.target_authored_event_code
                        })
                        .map(
                            |target_kfm_animation_declaration| AuthoredAnimationGraphEdge {
                                source_animation_graph_node_asset_key: kfm_animation_declaration
                                    .animation_clip_asset_key
                                    .clone(),
                                target_animation_graph_node_asset_key:
                                    target_kfm_animation_declaration
                                        .animation_clip_asset_key
                                        .clone(),
                                transition_animation_clip_asset_keys: Vec::new(),
                                authored_animation_attributes: vec![AuthoredAnimationAttribute {
                                    attribute_name: "blend".to_owned(),
                                    attribute_value: authored_transition
                                        .authored_blend_duration
                                        .clone(),
                                }],
                            },
                        )
                        .ok_or_else(|| {
                            invalid_kfm_record(
                                animation_set_asset_path,
                                &format!(
                                    "animation {} transitions to missing event code {}",
                                    kfm_animation_declaration.animation_clip_asset_key,
                                    authored_transition.target_authored_event_code,
                                ),
                            )
                        })
                })
        })
        .collect::<io::Result<Vec<_>>>()?;

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
