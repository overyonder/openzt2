use bevy::prelude::{KeyCode, Vec2};

use crate::application_lifecycle::GamePhase;

use super::verification_journey_types::{
    VerificationExpectedFactValue, VerificationFactComparison, VerificationFactExpectation,
    VerificationJourney, VerificationJourneyAction, VerificationJourneyStep,
    VerificationPointerTarget, VERIFICATION_TARGET_HEIGHT, VERIFICATION_TARGET_WIDTH,
};

pub(crate) fn read_verification_journey_file(
    path: &std::path::Path,
) -> std::io::Result<VerificationJourney> {
    let name = path.file_stem().map_or_else(
        || "journey".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let mut steps = Vec::new();
    append_journey_file_steps(path, &mut steps, 0)?;
    if steps.is_empty() {
        return Err(journey_script_error(path, 1, "journey has no steps"));
    }
    Ok(VerificationJourney {
        name,
        steps: steps.into_boxed_slice(),
    })
}

/// `include FILE` splices another step file, relative to the including file.
fn append_journey_file_steps(
    path: &std::path::Path,
    steps: &mut Vec<VerificationJourneyStep>,
    include_depth: u8,
) -> std::io::Result<()> {
    let source = std::fs::read_to_string(path)?;
    let file_name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    for (line_index, line) in source.lines().enumerate() {
        let source_line = line_index + 1;
        let tokens = split_journey_line_into_tokens(line)
            .map_err(|detail| journey_script_error(path, source_line, &detail))?;
        if tokens.is_empty() {
            continue;
        }
        // Log expectations are checked by the runner against the finished game log.
        if tokens[0] == "expect-log-absent" {
            continue;
        }
        if tokens[0] == "include" {
            let included = single(&tokens[1..])
                .map_err(|detail| journey_script_error(path, source_line, &detail))?;
            if include_depth >= 8 {
                return Err(journey_script_error(
                    path,
                    source_line,
                    "includes nest too deeply",
                ));
            }
            let included_path = path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join(included);
            append_journey_file_steps(&included_path, steps, include_depth + 1)?;
            continue;
        }
        let action = parse_journey_action(&tokens)
            .map_err(|detail| journey_script_error(path, source_line, &detail))?;
        steps.push(VerificationJourneyStep {
            source_location: format!("{file_name}:{source_line}"),
            action,
        });
    }
    Ok(())
}

/// Splits on whitespace, keeping double-quoted names whole and dropping `#` comments.
fn split_journey_line_into_tokens(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut characters = line.chars().peekable();
    while let Some(&character) = characters.peek() {
        if character.is_whitespace() {
            characters.next();
        } else if character == '#' {
            break;
        } else if character == '"' {
            characters.next();
            let mut token = String::new();
            loop {
                match characters.next() {
                    Some('"') => break,
                    Some(character) => token.push(character),
                    None => return Err("unterminated quoted name".to_owned()),
                }
            }
            tokens.push(token);
        } else {
            let mut token = String::new();
            while let Some(&character) = characters.peek() {
                if character.is_whitespace() || character == '#' {
                    break;
                }
                token.push(character);
                characters.next();
            }
            tokens.push(token);
        }
    }
    Ok(tokens)
}

fn parse_journey_action(tokens: &[String]) -> Result<VerificationJourneyAction, String> {
    let arguments = &tokens[1..];
    let action = match tokens[0].as_str() {
        "phase" => VerificationJourneyAction::WaitForPhase(parse_game_phase(single(arguments)?)?),
        "wait" => VerificationJourneyAction::WaitFrames(parse_number(single(arguments)?)?),
        "move" => VerificationJourneyAction::MovePointer(parse_pointer_target(arguments)?),
        "click" => VerificationJourneyAction::Click(parse_pointer_target(arguments)?),
        "click-row" => {
            let [list_name, index] = arguments else {
                return Err("expected `click-row \"List Name\" INDEX`".to_owned());
            };
            VerificationJourneyAction::Click(VerificationPointerTarget::ListRow {
                list_name: list_name.clone(),
                index: parse_number(index)?,
            })
        }
        "drag" => {
            if arguments.len() < 3 {
                return Err("drag needs a start target and an end position".to_owned());
            }
            let (from, to) = arguments.split_at(arguments.len() - 2);
            VerificationJourneyAction::Drag {
                from: parse_pointer_target(from)?,
                to: parse_target_position(&to[0], &to[1])?,
            }
        }
        "key" => match arguments {
            [key] => VerificationJourneyAction::Key {
                key_code: parse_key_code(key)?,
                repeat: 1,
            },
            [key, repeat] => VerificationJourneyAction::Key {
                key_code: parse_key_code(key)?,
                repeat: parse_number(repeat)?,
            },
            _ => return Err("expected `key KEY [REPEAT]`".to_owned()),
        },
        "type" => VerificationJourneyAction::TypeText(single(arguments)?.to_owned()),
        "wheel" => match arguments {
            [lines] => VerificationJourneyAction::Wheel {
                lines: parse_number(lines)?,
                repeat: 1,
            },
            [lines, repeat] => VerificationJourneyAction::Wheel {
                lines: parse_number(lines)?,
                repeat: parse_number(repeat)?,
            },
            _ => return Err("expected `wheel LINES [REPEAT]`".to_owned()),
        },
        "snapshot" => VerificationJourneyAction::Snapshot(single(arguments)?.to_owned()),
        "capture" => VerificationJourneyAction::Capture(single(arguments)?.to_owned()),
        "measure-frame-time" => {
            VerificationJourneyAction::MeasureFrameTime(parse_number(single(arguments)?)?)
        }
        "expect" => {
            let [fact, comparison, expected] = arguments else {
                return Err("expected `expect FACT OPERATOR VALUE`".to_owned());
            };
            VerificationJourneyAction::ExpectFact(VerificationFactExpectation {
                fact: fact.clone(),
                comparison: parse_fact_comparison(comparison)?,
                expected: match expected.strip_prefix('@') {
                    Some(snapshot) => VerificationExpectedFactValue::Snapshot(snapshot.to_owned()),
                    None => VerificationExpectedFactValue::Number(parse_number(expected)?),
                },
            })
        }
        "expect-rows" => {
            let [list_name, comparison, expected] = arguments else {
                return Err("expected `expect-rows \"List Name\" OPERATOR COUNT`".to_owned());
            };
            VerificationJourneyAction::ExpectListRowCount {
                list_name: list_name.clone(),
                comparison: parse_fact_comparison(comparison)?,
                expected: parse_number(expected)?,
            }
        }
        "expect-campaign-world" => {
            let [scenario, map] = arguments else {
                return Err("expected `expect-campaign-world SCENARIO MAP`".to_owned());
            };
            VerificationJourneyAction::ExpectCampaignWorld {
                scenario: scenario.clone(),
                map: map.clone(),
            }
        }
        "expect-text" => {
            let [node_name, expected] = arguments else {
                return Err("expected `expect-text \"Node Name\" \"text\"`".to_owned());
            };
            VerificationJourneyAction::ExpectNodeText {
                node_name: node_name.clone(),
                expected: expected.clone(),
            }
        }
        "expect-visible" | "expect-hidden" => VerificationJourneyAction::ExpectNodeVisibility {
            node_name: single(arguments)?.to_owned(),
            visible: tokens[0] == "expect-visible",
        },
        unknown => return Err(format!("unknown step `{unknown}`")),
    };
    Ok(action)
}

fn single(arguments: &[String]) -> Result<&str, String> {
    match arguments {
        [argument] => Ok(argument),
        _ => Err("expected exactly one argument".to_owned()),
    }
}

fn parse_number<T: std::str::FromStr>(token: &str) -> Result<T, String> {
    token
        .parse()
        .map_err(|_| format!("`{token}` is not a valid number"))
}

fn parse_pointer_target(arguments: &[String]) -> Result<VerificationPointerTarget, String> {
    match arguments {
        [x, y] if x.parse::<f32>().is_ok() => Ok(VerificationPointerTarget::Position(
            parse_target_position(x, y)?,
        )),
        [node_name] => Ok(VerificationPointerTarget::NamedNode {
            node_name: node_name.clone(),
            index: 0,
        }),
        [node_name, index] => Ok(VerificationPointerTarget::NamedNode {
            node_name: node_name.clone(),
            index: parse_number(index)?,
        }),
        _ => Err("expected `X Y` or `\"Node Name\" [INDEX]`".to_owned()),
    }
}

fn parse_target_position(x: &str, y: &str) -> Result<Vec2, String> {
    let position = Vec2::new(parse_number(x)?, parse_number(y)?);
    if (0.0..VERIFICATION_TARGET_WIDTH).contains(&position.x)
        && (0.0..VERIFICATION_TARGET_HEIGHT).contains(&position.y)
    {
        Ok(position)
    } else {
        Err(format!(
            "position must be inside the {VERIFICATION_TARGET_WIDTH}x{VERIFICATION_TARGET_HEIGHT} target"
        ))
    }
}

fn parse_game_phase(token: &str) -> Result<GamePhase, String> {
    Ok(match token {
        "splash" => GamePhase::Boot,
        "main-menu" => GamePhase::MainMenu,
        "map-selection" => GamePhase::MapSelection,
        "loading" => GamePhase::Loading,
        "in-game" => GamePhase::InGame,
        _ => return Err(format!("unknown phase `{token}`")),
    })
}

fn parse_fact_comparison(token: &str) -> Result<VerificationFactComparison, String> {
    Ok(match token {
        "==" => VerificationFactComparison::Equal,
        "!=" => VerificationFactComparison::NotEqual,
        ">" => VerificationFactComparison::Greater,
        ">=" => VerificationFactComparison::GreaterOrEqual,
        "<" => VerificationFactComparison::Less,
        "<=" => VerificationFactComparison::LessOrEqual,
        _ => return Err(format!("unknown comparison `{token}`")),
    })
}

fn parse_key_code(token: &str) -> Result<KeyCode, String> {
    Ok(match token {
        "ESCAPE" => KeyCode::Escape,
        "SPACE" => KeyCode::Space,
        "ENTER" => KeyCode::Enter,
        "DELETE" => KeyCode::Delete,
        "BACKSPACE" => KeyCode::Backspace,
        "TAB" => KeyCode::Tab,
        "HOME" => KeyCode::Home,
        "END" => KeyCode::End,
        "PAGE_UP" => KeyCode::PageUp,
        "PAGE_DOWN" => KeyCode::PageDown,
        "EQUAL" => KeyCode::Equal,
        "MINUS" => KeyCode::Minus,
        "LEFT" => KeyCode::ArrowLeft,
        "RIGHT" => KeyCode::ArrowRight,
        "UP" => KeyCode::ArrowUp,
        "DOWN" => KeyCode::ArrowDown,
        "CONTROL_L" => KeyCode::ControlLeft,
        "F1" => KeyCode::F1,
        "F8" => KeyCode::F8,
        "A" => KeyCode::KeyA,
        "D" => KeyCode::KeyD,
        "P" => KeyCode::KeyP,
        "S" => KeyCode::KeyS,
        "W" => KeyCode::KeyW,
        "Y" => KeyCode::KeyY,
        "Z" => KeyCode::KeyZ,
        _ => return Err(format!("unsupported key `{token}`")),
    })
}

fn journey_script_error(path: &std::path::Path, line: usize, detail: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{}:{line}: {detail}", path.display()),
    )
}
