use bevy::prelude::{GamepadButton, KeyCode, Vec2};

use openzt2_game_assets::application_lifecycle::GamePhase;

use super::verification_journey_types::{
    VerificationCondition, VerificationExpectedFactValue, VerificationFactComparison,
    VerificationFactExpectation, VerificationJourney, VerificationJourneyAction,
    VerificationJourneyStart, VerificationJourneyStep, VerificationNavigationRoutes,
    VerificationPointerTarget, VerificationSessionPlace, VERIFICATION_TARGET_HEIGHT,
    VERIFICATION_TARGET_WIDTH,
};

/// Frames an `until` step waits unless it names a limit: ten seconds at 60 Hz.
const UNTIL_DEFAULT_FRAME_LIMIT: u32 = 600;
/// Frames without a running UI animation before `until settled` holds.
const UNTIL_SETTLED_DEFAULT_FRAMES: u32 = 20;

/// A journey's first line is `start PLACE`; the rest are ordinary steps.
pub(crate) fn read_verification_journey_file(
    path: &std::path::Path,
) -> std::io::Result<VerificationJourney> {
    let name = path.file_stem().map_or_else(
        || "journey".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let source = std::fs::read_to_string(path)?;
    let (start_line, start_tokens) = source
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, split_journey_line_into_tokens(line)))
        .find(|(_, tokens)| tokens.as_ref().map_or(true, |tokens| !tokens.is_empty()))
        .ok_or_else(|| journey_script_error(path, 1, "journey has no steps"))?;
    let start_tokens =
        start_tokens.map_err(|detail| journey_script_error(path, start_line, &detail))?;
    let start = match start_tokens.split_first() {
        Some((keyword, arguments)) if keyword == "start" => parse_journey_start(arguments)
            .map_err(|detail| journey_script_error(path, start_line, &detail))?,
        _ => {
            return Err(journey_script_error(
                path,
                start_line,
                "a journey's first step must be `start PLACE`",
            ));
        }
    };
    let mut steps = Vec::new();
    // A journey may consist of its start alone, checked by its log expectations.
    append_journey_source_steps(path, &source, start_line, &mut steps)?;
    Ok(VerificationJourney {
        name,
        start,
        steps: steps.into_boxed_slice(),
    })
}

const BOOT_TO_MAIN_MENU: &str = r#"phase splash
until settled
click "splashbutton0"
phase main-menu
until settled
"#;

const IN_GAME_TO_MAIN_MENU: &str = r#"move "TextButton Return Hover"
until visible "TextButton Return To MainMenu"
click "TextButton Return To MainMenu"
until visible "return_mainmenu"
until settled
click "no_button"
phase main-menu
until settled
"#;

const MAP_SELECTION_TO_MAIN_MENU: &str = r#"click "Cancel Map"
phase main-menu
until settled
"#;

const MAIN_MENU_TO_FREEFORM_MAP: &str = r#"click "Freeform Button"
phase map-selection
click "alpine"
until rows "Map List" == 7
expect-text "Map Biome" "Alpine"
click-row "Map List" 0
until map_selected == 1
click "Play Map"
phase loading
until visible "Loading Screen"
expect-visible "Loading Bar"
capture loading
phase in-game
until visible "MainGUI"
expect-hidden "Loading Screen"
expect-hidden "Buy Info Panel"
expect unlimited_cash == 1
capture in-game
until placement_previews == 0
until construction_tool_active == 0
until settled
"#;

const MAIN_MENU_TO_CHALLENGE_MAP: &str = r#"click "Challenge Button"
phase map-selection
click "alpine"
until rows "Map List" == 7
expect-text "Map Biome" "Alpine"
click-row "Map List" 0
until map_selected == 1
click "Play Map"
phase in-game
until placement_previews == 0
until construction_tool_active == 0
until settled
"#;

/// Navigation shared by journey starts and explicit world cleanup.
pub(crate) fn verification_navigation_routes() -> std::io::Result<VerificationNavigationRoutes> {
    let read = |name: &str, source: &str| {
        let mut steps = Vec::new();
        append_journey_source_steps(std::path::Path::new(name), source, 0, &mut steps)?;
        Ok::<_, std::io::Error>(steps.into_boxed_slice())
    };
    Ok(VerificationNavigationRoutes {
        boot_to_main_menu: read("boot-to-main-menu", BOOT_TO_MAIN_MENU)?,
        in_game_to_main_menu: read("in-game-to-main-menu", IN_GAME_TO_MAIN_MENU)?,
        map_selection_to_main_menu: read("map-selection-to-main-menu", MAP_SELECTION_TO_MAIN_MENU)?,
        main_menu_to_freeform_map: read("main-menu-to-freeform-map", MAIN_MENU_TO_FREEFORM_MAP)?,
        main_menu_to_challenge_map: read("main-menu-to-challenge-map", MAIN_MENU_TO_CHALLENGE_MAP)?,
    })
}

fn parse_journey_start(arguments: &[String]) -> Result<VerificationJourneyStart, String> {
    let [place] = arguments else {
        return Err("expected `start PLACE`".to_owned());
    };
    let place = match place.as_str() {
        "boot" => VerificationSessionPlace::Boot,
        "main-menu" => VerificationSessionPlace::MainMenu,
        "freeform-map" => VerificationSessionPlace::FreeformMap,
        "challenge-map" => VerificationSessionPlace::ChallengeMap,
        _ => return Err(format!("unknown start `{place}`")),
    };
    Ok(VerificationJourneyStart { place })
}

/// Lines up to `skipped_lines` belong to the caller, such as a journey's `start`.
fn append_journey_source_steps(
    path: &std::path::Path,
    source: &str,
    skipped_lines: usize,
    steps: &mut Vec<VerificationJourneyStep>,
) -> std::io::Result<()> {
    let file_name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    for (line_index, line) in source.lines().enumerate().skip(skipped_lines) {
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
        if tokens[0] == "restore" {
            let start = parse_journey_start(&tokens[1..])
                .map_err(|detail| journey_script_error(path, source_line, &detail))?;
            if !matches!(
                start.place,
                VerificationSessionPlace::FreeformMap | VerificationSessionPlace::ChallengeMap
            ) {
                return Err(journey_script_error(
                    path,
                    source_line,
                    "restore needs the current map",
                ));
            }
            for action in [
                VerificationJourneyAction::ReturnInGameToKnownState(start.place),
                VerificationJourneyAction::MatchKnownState(start.place),
            ] {
                steps.push(VerificationJourneyStep {
                    source_location: format!("{file_name}:{source_line}"),
                    action,
                });
            }
            continue;
        }
        if tokens[0] == "return-to-menu" {
            if tokens.len() != 1 {
                return Err(journey_script_error(
                    path,
                    source_line,
                    "return-to-menu takes no arguments",
                ));
            }
            steps.push(VerificationJourneyStep {
                source_location: format!("{file_name}:{source_line}"),
                action: VerificationJourneyAction::ReturnInGameToOptions,
            });
            let first = steps.len();
            append_journey_source_steps(path, IN_GAME_TO_MAIN_MENU, 0, steps)?;
            for step in &mut steps[first..] {
                step.source_location = format!("{file_name}:{source_line}");
            }
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
        "open-game-options" if arguments.is_empty() => {
            VerificationJourneyAction::ReturnInGameToOptions
        }
        "phase" => VerificationJourneyAction::WaitForPhase(parse_game_phase(single(arguments)?)?),
        "wait" => VerificationJourneyAction::WaitFrames(parse_number(single(arguments)?)?),
        "until" => parse_until_step(arguments)?,
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
        "select-catalogue" => parse_catalogue_selection_step(arguments)?,
        "wheel-to-catalogue" => {
            let (definition_key, definition, list_name) = parse_catalogue_definition(arguments)?;
            VerificationJourneyAction::WheelToCatalogue {
                definition_key,
                definition,
                list_name,
            }
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
                key_code: parse_key_code(key.strip_prefix("CTRL+").unwrap_or(key))?,
                control: key.starts_with("CTRL+"),
                repeat: 1,
            },
            [key, repeat] => VerificationJourneyAction::Key {
                key_code: parse_key_code(key.strip_prefix("CTRL+").unwrap_or(key))?,
                control: key.starts_with("CTRL+"),
                repeat: parse_number(repeat)?,
            },
            _ => return Err("expected `key [CTRL+]KEY [REPEAT]`".to_owned()),
        },
        "hold" => {
            let [key, until, rest @ ..] = arguments else {
                return Err("expected `hold KEY until CONDITION [within FRAMES]`".to_owned());
            };
            if until != "until" {
                return Err("expected `hold KEY until CONDITION [within FRAMES]`".to_owned());
            }
            let VerificationJourneyAction::WaitUntil {
                condition,
                frame_limit,
            } = parse_until_step(rest)?
            else {
                return Err("hold needs a condition, not a settling wait".to_owned());
            };
            VerificationJourneyAction::HoldKeyUntil {
                key_code: parse_key_code(key)?,
                condition,
                frame_limit,
            }
        }
        "type" => VerificationJourneyAction::TypeText(single(arguments)?.to_owned()),
        "gamepad-connect" if arguments.is_empty() => VerificationJourneyAction::ConnectGamepad,
        "gamepad-button" => VerificationJourneyAction::GamepadButton(match single(arguments)? {
            "SOUTH" => GamepadButton::South,
            "EAST" => GamepadButton::East,
            button => return Err(format!("unsupported gamepad button `{button}`")),
        }),
        "gamepad-disconnect" if arguments.is_empty() => {
            VerificationJourneyAction::DisconnectGamepad
        }
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
        "expect" => VerificationJourneyAction::Expect(VerificationCondition::Fact(
            parse_fact_expectation(arguments)?,
        )),
        "expect-rows" => VerificationJourneyAction::Expect(parse_list_row_condition(arguments)?),
        "expect-campaign-world" => {
            let [scenario, map] = arguments else {
                return Err("expected `expect-campaign-world SCENARIO MAP`".to_owned());
            };
            VerificationJourneyAction::Expect(VerificationCondition::CampaignWorld {
                scenario: scenario.clone(),
                map: map.clone(),
            })
        }
        "expect-text" => VerificationJourneyAction::Expect(parse_node_text_condition(arguments)?),
        "expect-zoo-name" => {
            VerificationJourneyAction::Expect(VerificationCondition::WorldZooName {
                expected: single(arguments)?.to_owned(),
            })
        }
        "expect-visible" | "expect-hidden" => {
            VerificationJourneyAction::Expect(VerificationCondition::NodeVisibility {
                node_name: single(arguments)?.to_owned(),
                visible: tokens[0] == "expect-visible",
            })
        }
        unknown => return Err(format!("unknown step `{unknown}`")),
    };
    Ok(action)
}

/// `until CONDITION [within FRAMES]`, where CONDITION is `visible NAME`, `hidden NAME`,
/// `rows LIST OP COUNT`, `text NAME TEXT`, `FACT OP VALUE`, `steady FACT FRAMES` or `settled [FRAMES]`.
fn parse_until_step(arguments: &[String]) -> Result<VerificationJourneyAction, String> {
    let (arguments, frame_limit) = match arguments {
        [condition @ .., within, limit] if within == "within" => (condition, parse_number(limit)?),
        _ => (arguments, UNTIL_DEFAULT_FRAME_LIMIT),
    };
    if let Some((_, steady)) = arguments
        .split_first()
        .filter(|(kind, _)| *kind == "steady")
    {
        let [fact, steady_frames] = steady else {
            return Err("expected `until steady FACT FRAMES [within LIMIT]`".to_owned());
        };
        return Ok(VerificationJourneyAction::WaitUntilSteady {
            fact: fact.clone(),
            settled_value: None,
            steady_frames: parse_number(steady_frames)?,
            frame_limit,
        });
    }
    // No authored show or hide animation has run for the given frames.
    if let Some((_, settled)) = arguments
        .split_first()
        .filter(|(kind, _)| *kind == "settled")
    {
        return Ok(VerificationJourneyAction::WaitUntilSteady {
            fact: "ui_animations_running".to_owned(),
            settled_value: Some(0.0),
            steady_frames: match settled {
                [] => UNTIL_SETTLED_DEFAULT_FRAMES,
                [frames] => parse_number(frames)?,
                _ => return Err("expected `until settled [FRAMES] [within LIMIT]`".to_owned()),
            },
            frame_limit,
        });
    }
    let condition = match arguments.split_first() {
        Some((kind, condition)) if kind == "visible" || kind == "hidden" => {
            VerificationCondition::NodeVisibility {
                node_name: single(condition)?.to_owned(),
                visible: kind == "visible",
            }
        }
        Some((kind, condition)) if kind == "rows" => parse_list_row_condition(condition)?,
        Some((kind, condition)) if kind == "text" => parse_node_text_condition(condition)?,
        _ => VerificationCondition::Fact(parse_fact_expectation(arguments)?),
    };
    Ok(VerificationJourneyAction::WaitUntil {
        condition,
        frame_limit,
    })
}

/// `select-catalogue KEY` clicks the visible purchase row for the definition whose
/// source record key is KEY, canonicalised as the definitions are. Optional
/// `in "List Name"` scopes it to that list in the active in-game HUD.
fn parse_catalogue_selection_step(
    arguments: &[String],
) -> Result<VerificationJourneyAction, String> {
    let (definition_key, definition, list_name) = parse_catalogue_definition(arguments)?;
    Ok(VerificationJourneyAction::Click(
        VerificationPointerTarget::CatalogueEntry {
            definition_key,
            definition,
            list_name,
        },
    ))
}

fn parse_catalogue_definition(
    arguments: &[String],
) -> Result<(String, openzt2_game_data::AssetId, Option<String>), String> {
    let (definition_key, list_name) = match arguments {
        [definition_key] => (definition_key.clone(), None),
        [definition_key, scope, list_name] if scope == "in" => {
            (definition_key.clone(), Some(list_name.clone()))
        }
        _ => return Err("expected `KEY [in \"List Name\"]`".to_owned()),
    };
    let definition = openzt2_game_data::AssetId::from_key(
        &openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(
            &definition_key,
        ),
    );
    Ok((definition_key, definition, list_name))
}

fn parse_fact_expectation(arguments: &[String]) -> Result<VerificationFactExpectation, String> {
    let [fact, comparison, expected] = arguments else {
        return Err("expected `FACT OPERATOR VALUE`".to_owned());
    };
    Ok(VerificationFactExpectation {
        fact: fact.clone(),
        comparison: parse_fact_comparison(comparison)?,
        expected: match expected.strip_prefix('@') {
            Some(reference) => {
                let (reference, offset) = match reference.split_once('+') {
                    Some((reference, offset)) => (reference, parse_number(offset)?),
                    None => (reference, 0.0),
                };
                let (snapshot, snapshot_fact) =
                    reference.split_once('.').unwrap_or((reference, fact));
                VerificationExpectedFactValue::Snapshot {
                    snapshot: snapshot.to_owned(),
                    fact: snapshot_fact.to_owned(),
                    offset,
                }
            }
            None => VerificationExpectedFactValue::Number(parse_number(expected)?),
        },
    })
}

fn parse_list_row_condition(arguments: &[String]) -> Result<VerificationCondition, String> {
    let [list_name, comparison, expected] = arguments else {
        return Err("expected `\"List Name\" OPERATOR COUNT`".to_owned());
    };
    Ok(VerificationCondition::ListRowCount {
        list_name: list_name.clone(),
        comparison: parse_fact_comparison(comparison)?,
        expected: parse_number(expected)?,
    })
}

fn parse_node_text_condition(arguments: &[String]) -> Result<VerificationCondition, String> {
    let [node_name, expected] = arguments else {
        return Err("expected `\"Node Name\" \"text\"`".to_owned());
    };
    Ok(VerificationCondition::NodeText {
        node_name: node_name.clone(),
        expected: expected.clone(),
    })
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
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "F8" => KeyCode::F8,
        "A" => KeyCode::KeyA,
        "D" => KeyCode::KeyD,
        "E" => KeyCode::KeyE,
        "G" => KeyCode::KeyG,
        "P" => KeyCode::KeyP,
        "Q" => KeyCode::KeyQ,
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
