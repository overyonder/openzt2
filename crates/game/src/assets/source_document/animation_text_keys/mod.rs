use openzt2_game_data::animation::animation_text_key::{
    AuthoredAnimationSurfaceEffectKind, AuthoredAnimationTextAction, AuthoredAnimationTextCommand,
    AuthoredAnimationTextKey,
};

use crate::assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    blue_fang_source_document_parsing_error::BlueFangSourceDocumentParsingError,
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
    path::AssetPath,
};

pub(in crate::assets) fn parse_blue_fang_animation_text_key_sidecar_source(
    source_path: &str,
    source_bytes: &[u8],
) -> Result<Vec<AuthoredAnimationTextKey>, BlueFangSourceDocumentParsingError> {
    if blue_fang_animation_clip_binary_signature(source_bytes) {
        return Ok(Vec::new());
    }
    parse_blue_fang_source_document(AssetPath::new(source_path), source_bytes)
        .map(|document| parse_animation_text_keys_from_document(&document))
}

fn parse_animation_text_keys_from_document(
    document: &OrderedSourceDocument,
) -> Vec<AuthoredAnimationTextKey> {
    let mut keys = Vec::new();
    document
        .root
        .element_children()
        .for_each(|node| collect(node, &mut keys));
    keys
}

fn blue_fang_animation_clip_binary_signature(source_bytes: &[u8]) -> bool {
    source_bytes
        .get(..4)
        .and_then(|version_bytes| version_bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .is_some_and(|encoded_version| matches!(encoded_version, 1 | 2))
}

fn collect(node: &OrderedSourceDocumentNode, keys: &mut Vec<AuthoredAnimationTextKey>) {
    if node.name.eq_ignore_ascii_case("key") {
        let text = node.attribute("text").unwrap_or_default().to_owned();
        keys.push(AuthoredAnimationTextKey {
            authored_frame_number: number(node.attribute("frame")),
            authored_time_seconds: number(
                node.attribute("time")
                    .map(|value| value.trim_end_matches(['s', 'S'])),
            ),
            authored_playback_percentage: number(node.attribute("percent")),
            authored_text: text.clone(),
            animation_text_commands: split_commands(&text)
                .into_iter()
                .filter_map(parse_command)
                .collect(),
        });
    }
    node.element_children()
        .for_each(|child| collect(child, keys));
}

fn number(value: Option<&str>) -> Option<f32> {
    value?.trim().parse().ok()
}

fn split_commands(text: &str) -> Vec<&str> {
    let mut quote = None;
    let mut start = 0;
    let mut commands = Vec::new();
    for (index, character) in text.char_indices() {
        match (quote, character) {
            (Some(open), close) if open == close => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, ',') => {
                commands.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    commands.push(text[start..].trim());
    commands
}

fn parse_command(command: &str) -> Option<AuthoredAnimationTextCommand> {
    let (target_node, command) = command
        .strip_prefix('{')
        .and_then(|body| body.split_once('}'))
        .map_or((None, command.trim()), |(target, body)| {
            (Some(target.trim().to_owned()), body.trim())
        });
    if command.is_empty() {
        return None;
    }
    let immediate = command.starts_with('!');
    let command = command.trim_start_matches('!').trim();
    let lower = command.to_ascii_lowercase();
    let rest = |prefix: &str| command.get(prefix.len()..).map(str::trim);
    let token = |value: &str| first_quoted_or_word(value).unwrap_or_default().to_owned();
    let option = |name: &str| option_f32(command, name);
    let action = if lower.starts_with("snd ") || lower.starts_with("and ") {
        let body = rest("snd").or_else(|| rest("and"))?;
        AuthoredAnimationTextAction::PlaySound {
            audio_asset_key: token(body),
            use_terrain_surface_sound: lower.contains(" terrain"),
            playback_is_looped: lower.contains(" looping"),
            update_sound_position_during_playback: lower.contains(" updatepos"),
        }
    } else if lower.starts_with("runps ") {
        AuthoredAnimationTextAction::RunParticleSystem {
            particle_system_asset_key: token(rest("runps")?),
            use_target_z_axis_rotation: lower.contains(" usezrot"),
            particle_scale: option("scale"),
        }
    } else if lower.starts_with("splash ") {
        AuthoredAnimationTextAction::TriggerSurfaceEffect {
            surface_effect_kind: AuthoredAnimationSurfaceEffectKind::NamedSurfaceEffect,
            effect_strength: option("strength"),
            named_surface_effect_asset_key: Some(token(rest("splash")?)),
        }
    } else if ["runspl", "runspr", "runsns"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        let kind = if lower.starts_with("runspr") {
            AuthoredAnimationSurfaceEffectKind::DirectionalVector
        } else if lower.starts_with("runsns") {
            AuthoredAnimationSurfaceEffectKind::DirectionalVectorAndSurfaceImpact
        } else {
            AuthoredAnimationSurfaceEffectKind::SurfaceImpact
        };
        AuthoredAnimationTextAction::TriggerSurfaceEffect {
            surface_effect_kind: kind,
            effect_strength: lower
                .strip_prefix("runspl")
                .or_else(|| lower.strip_prefix("runspr"))
                .or_else(|| lower.strip_prefix("runsns"))
                .and_then(|suffix| suffix.trim().parse().ok())
                .or_else(|| option("strength")),
            named_surface_effect_asset_key: None,
        }
    } else if lower == "attach object" {
        AuthoredAnimationTextAction::AttachPendingObject
    } else if lower.starts_with("attach ") {
        AuthoredAnimationTextAction::AttachNamedObject {
            object_asset_key: token(rest("attach")?),
        }
    } else if lower == "detach object" || lower == "detach_object" {
        AuthoredAnimationTextAction::DetachObject
    } else if lower == "spawn object" {
        AuthoredAnimationTextAction::SpawnObject
    } else if lower.starts_with("create ") {
        AuthoredAnimationTextAction::CreateObject {
            object_asset_key: rest("create")?.to_owned(),
        }
    } else if lower.starts_with("destroy ") {
        AuthoredAnimationTextAction::DestroyObject {
            object_asset_key: rest("destroy")?.to_owned(),
        }
    } else if lower.starts_with("anim ") {
        AuthoredAnimationTextAction::PlayAnimationGraphNode {
            animation_graph_node_asset_key: token(rest("anim")?),
            blend_duration_seconds: option("blend"),
            advance_current_animation_time: lower.contains("advance_cur_t"),
            enter_immediately: immediate,
        }
    } else if lower == "dis" {
        AuthoredAnimationTextAction::SetAnimationGraphEnabled(false)
    } else if lower == "en" {
        AuthoredAnimationTextAction::SetAnimationGraphEnabled(true)
    } else if lower == "exit" {
        AuthoredAnimationTextAction::ExitAnimation
    } else if lower.starts_with("gf ") || lower.starts_with("gf=") {
        AuthoredAnimationTextAction::SetGroundFit(
            lower[2..].trim_start_matches('=').trim().parse().ok()?,
        )
    } else if lower.starts_with("dc_enabled ") {
        AuthoredAnimationTextAction::SetDockControllerEnabled(parse_bool(rest("dc_enabled")?)?)
    } else if lower.starts_with("dc_goto ") {
        AuthoredAnimationTextAction::GoToDockControllerNode(rest("dc_goto")?.parse().ok()?)
    } else if lower.starts_with("ai ") {
        AuthoredAnimationTextAction::SendArtificialIntelligenceCommand(rest("ai")?.to_owned())
    } else if lower.starts_with("as ") {
        let mut values = rest("as")?.split_ascii_whitespace();
        AuthoredAnimationTextAction::SetAnimationPlaybackSpeed {
            playback_speed_percent: values.next()?.parse().ok()?,
            authored_speed_mode: values.next()?.parse().ok()?,
        }
    } else if lower.starts_with("whap ") {
        let mut values = rest("whap")?.split_ascii_whitespace();
        AuthoredAnimationTextAction::ApplyWhapImpulse {
            impulse_angle_degrees: values.next()?.parse().ok()?,
            impulse_duration_frames: values
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
        }
    } else if lower.starts_with("runpeff ") {
        let body = rest("runpeff")?;
        AuthoredAnimationTextAction::RunPresentationEffect {
            presentation_effect_asset_key: token(body),
            attach_to_target: lower.contains(" attach"),
            stop_attached_effects: lower.contains(" stop"),
            disable_distance_culling: lower.contains(" nocull"),
            water_height_offset: option_f32(body, "water"),
        }
    } else if lower.starts_with("attachps ") {
        AuthoredAnimationTextAction::AttachParticleSystem {
            particle_system_asset_key: token(rest("attachps")?),
            use_target_rotation: lower.contains(" userot"),
        }
    } else if lower.starts_with("detachps ") {
        AuthoredAnimationTextAction::DetachParticleSystem {
            particle_system_asset_key: token(rest("detachps")?),
        }
    } else if lower == "kill" {
        AuthoredAnimationTextAction::KillAnimationSubject
    } else if lower.starts_with("tap-trans ") {
        AuthoredAnimationTextAction::RequestTraversalTransition {
            animation_graph_node_asset_key: token(rest("tap-trans")?),
        }
    } else if lower.starts_with("createoverride ") {
        AuthoredAnimationTextAction::CreateObjectOverride {
            object_asset_key: token(rest("createoverride")?),
        }
    } else if lower.starts_with("start ") {
        let body = rest("start")?;
        AuthoredAnimationTextAction::StartAlignedAnimation {
            animation_graph_node_asset_key: flag(body, "-name")?.to_owned(),
            authored_actor_range: flag(body, "-ar").unwrap_or_default().to_owned(),
            authored_alignment_axes: flag(body, "-at").unwrap_or_default().to_owned(),
            authored_front_axis: flag(body, "-front").unwrap_or_default().to_owned(),
            playback_is_looped: body.split_ascii_whitespace().any(|part| part == "-loop"),
        }
    } else if !command.chars().any(char::is_whitespace) {
        AuthoredAnimationTextAction::RequestAnimationGraphNode(command.to_owned())
    } else {
        AuthoredAnimationTextAction::UnrecognizedAuthoredCommand(command.to_owned())
    };
    Some(AuthoredAnimationTextCommand {
        target_skeleton_joint_asset_key: target_node,
        animation_text_action: action,
    })
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

fn first_quoted_or_word(value: &str) -> Option<&str> {
    let value = value.trim();
    if let Some(quote) = value
        .chars()
        .next()
        .filter(|character| matches!(character, '\'' | '"'))
    {
        return value
            .get(quote.len_utf8()..)?
            .split_once(quote)
            .map(|(token, _)| token);
    }
    value.split_ascii_whitespace().next()
}

fn option_f32(command: &str, name: &str) -> Option<f32> {
    command
        .split_ascii_whitespace()
        .find_map(|part| {
            let (key, value) = part.split_once('=')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.parse().ok())
                .flatten()
        })
        .or_else(|| {
            let mut words = command.split_ascii_whitespace();
            while let Some(word) = words.next() {
                if word.eq_ignore_ascii_case(name) {
                    return words.next()?.parse().ok();
                }
            }
            None
        })
}

fn flag<'a>(command: &'a str, name: &str) -> Option<&'a str> {
    let mut words = command.split_ascii_whitespace();
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case(name) {
            return words.next();
        }
    }
    None
}
