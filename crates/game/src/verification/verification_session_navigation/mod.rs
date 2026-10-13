//! Moves one game between journey starts through its own UI, and decides when
//! the session must fall back to the main menu or to a fresh game.

use bevy::{camera::NormalizedRenderTarget, ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::plugins::{
    construction::{
        construction_interaction_types::ConstructionPreview,
        construction_tool_and_placement_policy_types::ConstructionTool,
    },
    immersive_modes::immersive_mode_state_types::ActiveImmersiveMode,
    simulation_time::simulation_control_types::SimulationControl,
    ui::{
        authored_modal_presentation::UiAuthoredModalPresentation,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
    },
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};

use super::{
    verification_input_injection::VerificationInputInjection,
    verification_journey_execution::{VerificationStepProgress, VerificationUiNodeQuery},
    verification_journey_state_types::{
        VerificationDismissalAfterRelease, VerificationDismissalProgress,
        VerificationDismissalWait, VerificationFacts, VerificationJourneySession,
        VerificationKnownState, VerificationSessionStage, VerificationUiFingerprint,
    },
    verification_journey_types::{
        VerificationJourneyAction, VerificationJourneyStep, VerificationSessionPlace,
    },
    verification_pointer_reach::VerificationPointerReach,
};

/// Facts that must match a place's first arrival before a journey can reuse it.
/// Guests and cash move with the simulation, so they are left out.
const KNOWN_STATE_FACTS: &[&str] = &[
    "phase",
    "selected",
    "construction_tool_active",
    "placement_previews",
    "immersive_mode_active",
    "simulation_paused",
    "unlimited_cash",
    "camera_x_m",
    "camera_z_m",
    "camera_height_m",
    "camera_pitch_deg",
    "camera_fov_deg",
    "fences",
    "habitats",
    "animals",
    "staff",
    "facilities",
    "placed_objects",
    "paths",
    "photos",
    "research_projects",
];
const KNOWN_STATE_FACT_TOLERANCE: f64 = 1e-3;
const ESCAPE_PRESS_LIMIT: u32 = 8;
/// Frames to wait for any visible response to Escape or a close control.
const DISMISSAL_RESPONSE_FRAME_LIMIT: u32 = 90;
/// The options overlay closes when its hide animation ends.
const OPTIONS_CLOSE_FRAME_LIMIT: u32 = 180;
/// Frames for show and hide animations to finish before the next assessment.
const DISMISSAL_SETTLE_FRAMES: u32 = 20;
/// Controls tried, in order, when Escape changes nothing.
const IN_GAME_DISMISS_CONTROLS: &[&str] = &["close", "cancel", "TextButton Cancel"];
const MAIN_MENU_DISMISS_CONTROLS: &[&str] = &["Back", "TextButton Cancel", "cancel", "close"];

#[derive(SystemParam)]
pub(crate) struct VerificationSessionObservation<'w, 's> {
    phase: Res<'w, State<GamePhase>>,
    roots: Query<'w, 's, (&'static InheritedVisibility, &'static UiDocumentRoot)>,
    modal_nodes: Query<
        'w,
        's,
        (
            &'static UiDocumentOwner,
            &'static InheritedVisibility,
            &'static UiAuthoredModalPresentation,
        ),
    >,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    simulation: Option<Res<'w, SimulationControl>>,
    construction_tool: Option<Res<'w, ConstructionTool>>,
    immersive_modes: Query<'w, 's, (), With<ActiveImmersiveMode>>,
    previews: Query<'w, 's, (), With<ConstructionPreview>>,
}

impl VerificationSessionObservation<'_, '_> {
    pub(crate) fn visible_roles(&self) -> Vec<UiDocumentRole> {
        let mut roles = self
            .roots
            .iter()
            .filter(|(visibility, _)| visibility.get())
            .filter_map(|(_, root)| self.documents.get(&root.document))
            .map(|document| document.canonical_ui_document().role)
            .collect::<Vec<_>>();
        roles.sort_unstable();
        roles.dedup();
        roles
    }

    /// Visible roles that describe the game's state. The application root and
    /// fragments such as tooltips come and go without any change of state.
    pub(crate) fn significant_visible_roles(&self) -> Vec<UiDocumentRole> {
        let mut roles = self.visible_roles();
        roles.retain(|role| {
            // These documents keep an empty root alive after their authored
            // modal panels close. Cache lifetime is not visible game state.
            if matches!(
                role,
                UiDocumentRole::ZooStatus | UiDocumentRole::Scenario | UiDocumentRole::Zoopedia
            ) {
                return self.modal_nodes.iter().any(|(owner, visibility, modal)| {
                    modal.is_modal()
                        && visibility.get()
                        && self.roots.get(owner.0).is_ok_and(|(_, root)| {
                            self.documents.get(&root.document).is_some_and(|document| {
                                document.canonical_ui_document().role == *role
                            })
                        })
                });
            }
            !matches!(
                role,
                UiDocumentRole::ApplicationRoot | UiDocumentRole::Fragment
            )
        });
        roles
    }

    fn simulation_paused(&self) -> bool {
        self.simulation
            .as_ref()
            .is_some_and(|simulation| simulation.paused)
    }

    fn fingerprint(&self, nodes: &VerificationUiNodeQuery) -> VerificationUiFingerprint {
        VerificationUiFingerprint {
            visible_roles: self.visible_roles(),
            visible_named_nodes: nodes
                .iter()
                .filter(|(_, _, _, visibility, _)| visibility.get())
                .count(),
            construction_tool_active: self
                .construction_tool
                .as_ref()
                .is_some_and(|tool| **tool != ConstructionTool::Inspect),
            immersive_mode_active: !self.immersive_modes.is_empty(),
            placement_previews: self.previews.iter().count(),
        }
    }
}

/// Plans the stage that brings the game to the current journey's start.
pub(crate) fn choose_route_to_current_journey_start(
    session: &mut VerificationJourneySession,
    phase: GamePhase,
) -> Result<(VerificationSessionStage, Vec<VerificationJourneyStep>), String> {
    let start = session.journeys[session.journey_index].start;
    let session_step = |action| VerificationJourneyStep {
        source_location: "session".to_owned(),
        action,
    };
    match start.place {
        VerificationSessionPlace::Boot if phase == GamePhase::Boot => {
            Ok((VerificationSessionStage::RouteThroughMainMenu, Vec::new()))
        }
        VerificationSessionPlace::Boot => {
            Err("a journey that starts at boot needs a freshly launched game".to_owned())
        }
        VerificationSessionPlace::MainMenu => {
            let mut steps = route_steps_to_main_menu(session, phase);
            steps.push(session_step(
                VerificationJourneyAction::ReturnToMainMenuScreen,
            ));
            steps.push(session_step(VerificationJourneyAction::MatchKnownState(
                VerificationSessionPlace::MainMenu,
            )));
            Ok((VerificationSessionStage::RouteThroughMainMenu, steps))
        }
        map => {
            let map_is_loaded = session.place == Some((map, session.world_loads_started))
                && phase == GamePhase::InGame;
            if map_is_loaded {
                return Ok((
                    VerificationSessionStage::ResetWithinMap,
                    vec![
                        session_step(VerificationJourneyAction::ReturnInGameToKnownState(map)),
                        session_step(VerificationJourneyAction::MatchKnownState(map)),
                    ],
                ));
            }
            Ok((
                VerificationSessionStage::RouteThroughMainMenu,
                route_steps_to_new_map(session, phase, map),
            ))
        }
    }
}

/// Leaves wherever the game is for the main menu, then loads `map` afresh.
pub(crate) fn route_steps_to_new_map(
    session: &mut VerificationJourneySession,
    phase: GamePhase,
    map: VerificationSessionPlace,
) -> Vec<VerificationJourneyStep> {
    let session_step = |action| VerificationJourneyStep {
        source_location: "session".to_owned(),
        action,
    };
    // A new world has a new known state, recorded on arrival.
    session.known_states.remove(&map);
    let mut steps = route_steps_to_main_menu(session, phase);
    steps.push(session_step(
        VerificationJourneyAction::ReturnToMainMenuScreen,
    ));
    steps.push(session_step(VerificationJourneyAction::MatchKnownState(
        VerificationSessionPlace::MainMenu,
    )));
    steps.extend_from_slice(match map {
        VerificationSessionPlace::ChallengeMap => &session.routes.main_menu_to_challenge_map,
        _ => &session.routes.main_menu_to_freeform_map,
    });
    // A freshly loaded map is recorded as it opens; pressing Escape here could
    // open the in-game options.
    steps.push(session_step(VerificationJourneyAction::MatchKnownState(
        map,
    )));
    steps
}

fn route_steps_to_main_menu(
    session: &VerificationJourneySession,
    phase: GamePhase,
) -> Vec<VerificationJourneyStep> {
    let session_step = |action| VerificationJourneyStep {
        source_location: "session".to_owned(),
        action,
    };
    let routes = &session.routes;
    match phase {
        GamePhase::Boot => routes.boot_to_main_menu.to_vec(),
        GamePhase::MainMenu => Vec::new(),
        GamePhase::MapSelection => routes.map_selection_to_main_menu.to_vec(),
        GamePhase::Loading | GamePhase::InGame => {
            let mut steps = vec![
                session_step(VerificationJourneyAction::WaitForPhase(GamePhase::InGame)),
                session_step(VerificationJourneyAction::ReturnInGameToOptions),
            ];
            steps.extend_from_slice(&routes.in_game_to_main_menu);
            steps
        }
    }
}

/// The main menu with none of its screens or dialogs open.
fn main_menu_is_at_rest(visible_roles: &[UiDocumentRole]) -> bool {
    visible_roles.contains(&UiDocumentRole::MainMenu)
        && visible_roles.iter().all(|role| {
            matches!(
                role,
                UiDocumentRole::ApplicationRoot
                    | UiDocumentRole::MainMenu
                    | UiDocumentRole::MainMenuBackdrop
                    | UiDocumentRole::Fragment
            )
        })
}

/// Records a map's state on first arrival and compares later arrivals with it.
/// The main menu is compared with its resting state instead.
pub(crate) fn match_known_state(
    session: &mut VerificationJourneySession,
    place: VerificationSessionPlace,
    visible_roles: Vec<UiDocumentRole>,
    facts: &VerificationFacts,
) -> Result<(), String> {
    if place == VerificationSessionPlace::MainMenu {
        return if main_menu_is_at_rest(&visible_roles) {
            Ok(())
        } else {
            Err(format!("the main menu shows {visible_roles:?}"))
        };
    }
    let current = VerificationKnownState {
        visible_roles,
        facts: KNOWN_STATE_FACTS
            .iter()
            .filter_map(|name| facts.get(*name).map(|value| ((*name).to_owned(), *value)))
            .collect(),
    };
    let Some(known) = session.known_states.get(&place) else {
        info!(target: "openzt2_verification", ?place, visible_roles = ?current.visible_roles,
            facts = ?current.facts, "verification known state recorded");
        session.known_states.insert(place, current);
        return Ok(());
    };
    let mut differences = Vec::new();
    if known.visible_roles != current.visible_roles {
        differences.push(format!(
            "visible documents {:?} instead of {:?}",
            current.visible_roles, known.visible_roles
        ));
    }
    for (name, expected) in &known.facts {
        match current.facts.get(name) {
            Some(actual) if (actual - expected).abs() <= KNOWN_STATE_FACT_TOLERANCE => {}
            actual => differences.push(format!("{name} {actual:?} instead of {expected}")),
        }
    }
    if differences.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{place:?} differs from its known state: {}",
            differences.join("; ")
        ))
    }
}

/// Without a place, presses Escape until the in-game options open, ready to
/// leave the map. With a place, dismisses activities until the visible documents
/// match that map's known state, and resumes a simulation that was running.
/// Escape in the plain overhead view opens the in-game options, so it is pressed
/// only while a tool, preview or immersive mode owns it; otherwise a visible
/// close control is clicked.
#[allow(clippy::too_many_arguments)]
pub(crate) fn advance_in_game_dismissal(
    session: &mut VerificationJourneySession,
    observation: &VerificationSessionObservation,
    nodes: &VerificationUiNodeQuery,
    reach: &VerificationPointerReach,
    input: &mut VerificationInputInjection,
    target: &NormalizedRenderTarget,
    frame: u32,
    known_place: Option<VerificationSessionPlace>,
) -> VerificationStepProgress {
    let visible_roles = observation.visible_roles();
    let options_visible = visible_roles.contains(&UiDocumentRole::InGameOptions);
    let known = known_place.and_then(|place| session.known_states.get(&place));
    let known_roles = known.map(|known| known.visible_roles.clone());
    let significant_roles = observation.significant_visible_roles();
    let fingerprint = observation.fingerprint(nodes);
    let escape_has_an_owner = fingerprint.construction_tool_active
        || fingerprint.immersive_mode_active
        || fingerprint.placement_previews > 0;
    let known_running = known.is_some_and(|known| {
        known
            .facts
            .get("simulation_paused")
            .is_some_and(|paused| *paused == 0.0)
    });
    let progress = &mut session.step_dismissal;
    match progress.wait.clone() {
        VerificationDismissalWait::Assess => {
            if *observation.phase.get() != GamePhase::InGame {
                return VerificationStepProgress::Failed(format!(
                    "the game left the map for {:?} while dismissing in-game activities",
                    observation.phase.get()
                ));
            }
            let Some(known_roles) = known_roles else {
                if options_visible {
                    return VerificationStepProgress::Complete;
                }
                if progress.attempts >= ESCAPE_PRESS_LIMIT {
                    return VerificationStepProgress::Failed(format!(
                        "Escape did not reach the in-game options after {ESCAPE_PRESS_LIMIT} presses; visible documents {visible_roles:?}"
                    ));
                }
                press_escape_and_await_response(progress, observation, nodes, input);
                return VerificationStepProgress::Running;
            };
            if options_visible && !known_roles.contains(&UiDocumentRole::InGameOptions) {
                info!(target: "openzt2_verification", ?visible_roles,
                    "verification dismissal pressed Escape to close the in-game options");
                input.press_or_release_key(KeyCode::Escape, true);
                progress.wait = VerificationDismissalWait::ReleaseKey {
                    key_code: KeyCode::Escape,
                    then: VerificationDismissalAfterRelease::OptionsClosed,
                };
                return VerificationStepProgress::Running;
            }
            if significant_roles == known_roles && !escape_has_an_owner {
                if known_running && observation.simulation_paused() {
                    input.press_or_release_key(KeyCode::KeyP, true);
                    progress.wait = VerificationDismissalWait::ReleaseKey {
                        key_code: KeyCode::KeyP,
                        then: VerificationDismissalAfterRelease::SimulationResumed,
                    };
                    return VerificationStepProgress::Running;
                }
                return VerificationStepProgress::Complete;
            }
            if progress.attempts >= ESCAPE_PRESS_LIMIT {
                return VerificationStepProgress::Failed(format!(
                    "the map still shows {significant_roles:?} instead of {known_roles:?} after {ESCAPE_PRESS_LIMIT} dismissals"
                ));
            }
            if escape_has_an_owner {
                press_escape_and_await_response(progress, observation, nodes, input);
                return VerificationStepProgress::Running;
            }
            let candidates = dismiss_control_candidates(nodes, reach, IN_GAME_DISMISS_CONTROLS);
            let Some((control, click)) =
                click_first_dismiss_control(progress, candidates, frame, false)
            else {
                return VerificationStepProgress::Failed(format!(
                    "the map shows {significant_roles:?} instead of {known_roles:?}, with no tool or close control to dismiss"
                ));
            };
            progress.attempts += 1;
            progress.fingerprint_before = fingerprint;
            info!(target: "openzt2_verification", control, visible_roles = ?significant_roles,
                "verification dismissal clicking a close control");
            progress.wait = click;
            VerificationStepProgress::Running
        }
        VerificationDismissalWait::OptionsClosed {
            since_frame,
            clicked_return_to_game,
        } => {
            if !options_visible {
                progress.wait = VerificationDismissalWait::Settle {
                    until_frame: frame + DISMISSAL_SETTLE_FRAMES,
                };
                return VerificationStepProgress::Running;
            }
            if frame - since_frame < OPTIONS_CLOSE_FRAME_LIMIT {
                return VerificationStepProgress::Running;
            }
            let candidates = if clicked_return_to_game {
                Vec::new()
            } else {
                dismiss_control_candidates(nodes, reach, &["TextButton Cancel Hover"])
            };
            let Some((_, click)) = click_first_dismiss_control(progress, candidates, frame, true)
            else {
                return VerificationStepProgress::Failed(format!(
                    "neither Escape nor Return to Game closed the in-game options within {OPTIONS_CLOSE_FRAME_LIMIT} frames"
                ));
            };
            info!(target: "openzt2_verification", visible_roles = ?observation.visible_roles(),
                "verification dismissal found Escape did not close the in-game options; clicking Return to Game");
            progress.wait = click;
            VerificationStepProgress::Running
        }
        VerificationDismissalWait::SimulationResumed { since_frame } => {
            if !observation.simulation_paused() {
                VerificationStepProgress::Complete
            } else if frame - since_frame >= DISMISSAL_RESPONSE_FRAME_LIMIT {
                VerificationStepProgress::Failed("P did not resume the simulation".to_owned())
            } else {
                VerificationStepProgress::Running
            }
        }
        wait => advance_shared_dismissal_wait(
            wait,
            session,
            observation,
            nodes,
            reach,
            input,
            target,
            frame,
            IN_GAME_DISMISS_CONTROLS,
        ),
    }
}

/// Dismisses main-menu screens and dialogs until the main menu is at rest.
pub(crate) fn advance_main_menu_dismissal(
    session: &mut VerificationJourneySession,
    observation: &VerificationSessionObservation,
    nodes: &VerificationUiNodeQuery,
    reach: &VerificationPointerReach,
    input: &mut VerificationInputInjection,
    target: &NormalizedRenderTarget,
    frame: u32,
) -> VerificationStepProgress {
    match session.step_dismissal.wait.clone() {
        VerificationDismissalWait::Assess => {
            if *observation.phase.get() != GamePhase::MainMenu {
                return VerificationStepProgress::Failed(format!(
                    "expected the main menu, found {:?}",
                    observation.phase.get()
                ));
            }
            let visible_roles = observation.visible_roles();
            if main_menu_is_at_rest(&visible_roles) {
                return VerificationStepProgress::Complete;
            }
            if session.step_dismissal.attempts >= ESCAPE_PRESS_LIMIT {
                return VerificationStepProgress::Failed(format!(
                    "the main menu still shows {visible_roles:?} after {ESCAPE_PRESS_LIMIT} dismissals"
                ));
            }
            press_escape_and_await_response(&mut session.step_dismissal, observation, nodes, input);
            VerificationStepProgress::Running
        }
        wait => advance_shared_dismissal_wait(
            wait,
            session,
            observation,
            nodes,
            reach,
            input,
            target,
            frame,
            MAIN_MENU_DISMISS_CONTROLS,
        ),
    }
}

fn press_escape_and_await_response(
    progress: &mut VerificationDismissalProgress,
    observation: &VerificationSessionObservation,
    nodes: &VerificationUiNodeQuery,
    input: &mut VerificationInputInjection,
) {
    progress.attempts += 1;
    progress.fingerprint_before = observation.fingerprint(nodes);
    info!(target: "openzt2_verification", attempt = progress.attempts,
        visible_roles = ?progress.fingerprint_before.visible_roles,
        "verification dismissal pressed Escape");
    input.press_or_release_key(KeyCode::Escape, true);
    progress.wait = VerificationDismissalWait::ReleaseKey {
        key_code: KeyCode::Escape,
        then: VerificationDismissalAfterRelease::Response,
    };
}

#[allow(clippy::too_many_arguments)]
fn advance_shared_dismissal_wait(
    wait: VerificationDismissalWait,
    session: &mut VerificationJourneySession,
    observation: &VerificationSessionObservation,
    nodes: &VerificationUiNodeQuery,
    reach: &VerificationPointerReach,
    input: &mut VerificationInputInjection,
    target: &NormalizedRenderTarget,
    frame: u32,
    dismiss_controls: &[&'static str],
) -> VerificationStepProgress {
    let next_wait = match wait {
        VerificationDismissalWait::ReleaseKey { key_code, then } => {
            input.press_or_release_key(key_code, false);
            match then {
                VerificationDismissalAfterRelease::Response => {
                    VerificationDismissalWait::Response {
                        since_frame: frame,
                        clicked: false,
                    }
                }
                VerificationDismissalAfterRelease::OptionsClosed => {
                    VerificationDismissalWait::OptionsClosed {
                        since_frame: frame,
                        clicked_return_to_game: false,
                    }
                }
                VerificationDismissalAfterRelease::SimulationResumed => {
                    VerificationDismissalWait::SimulationResumed { since_frame: frame }
                }
            }
        }
        VerificationDismissalWait::Response {
            since_frame,
            clicked,
        } => {
            if observation.fingerprint(nodes) != session.step_dismissal.fingerprint_before {
                VerificationDismissalWait::Settle {
                    until_frame: frame + DISMISSAL_SETTLE_FRAMES,
                }
            } else if frame - since_frame < DISMISSAL_RESPONSE_FRAME_LIMIT {
                wait
            } else if let Some((control, click)) = if clicked {
                None
            } else {
                click_first_dismiss_control(
                    &mut session.step_dismissal,
                    dismiss_control_candidates(nodes, reach, dismiss_controls),
                    frame,
                    false,
                )
            } {
                info!(target: "openzt2_verification", control,
                    visible_roles = ?observation.visible_roles(),
                    "verification dismissal found Escape changed nothing; clicking a dismiss control");
                click
            } else {
                return VerificationStepProgress::Failed(format!(
                    "neither Escape nor a close control changed anything; visible documents {:?}",
                    observation.visible_roles()
                ));
            }
        }
        // Hover buttons reveal their clickable control a few frames after the pointer arrives.
        VerificationDismissalWait::ClickControl {
            node,
            position,
            started_frame,
            closes_options,
        } => match frame - started_frame {
            0 => {
                input.move_pointer(target, session.pointer_position, position);
                session.pointer_position = position;
                wait
            }
            // A same-named control a press would reach is preferred to one it would not.
            1 if !reach.pointer_reaches(node)
                && !session.step_dismissal.dismiss_alternatives.is_empty() =>
            {
                let (node, position) = session.step_dismissal.dismiss_alternatives.remove(0);
                VerificationDismissalWait::ClickControl {
                    node,
                    position,
                    started_frame: frame + 1,
                    closes_options,
                }
            }
            10 if !reach.pointer_reaches(node) => {
                return VerificationStepProgress::Failed(
                    "the dismissal control is covered at press time".to_owned(),
                );
            }
            10 if !nodes
                .get(node)
                .is_ok_and(|(_, _, transform, visible, enabled)| {
                    visible.get()
                        && enabled.is_none_or(|enabled| enabled.0)
                        && transform.translation.distance(position) <= 0.5
                }) =>
            {
                return VerificationStepProgress::Failed(
                    "the dismissal control moved or became unavailable before its press".to_owned(),
                );
            }
            elapsed @ (10 | 12) => {
                input.press_or_release_primary_button(target, position, elapsed == 10);
                wait
            }
            14.. if closes_options => VerificationDismissalWait::OptionsClosed {
                since_frame: frame,
                clicked_return_to_game: true,
            },
            // Compare with the UI before the click, which may already have responded.
            14.. => VerificationDismissalWait::Response {
                since_frame: frame,
                clicked: true,
            },
            _ => wait,
        },
        VerificationDismissalWait::Settle { until_frame } if frame >= until_frame => {
            VerificationDismissalWait::Assess
        }
        other => other,
    };
    session.step_dismissal.wait = next_wait;
    VerificationStepProgress::Running
}

/// Visible, enabled nodes with one of the dismiss names, in the names' order and
/// then top-to-bottom and left-to-right.
fn dismiss_control_candidates(
    nodes: &VerificationUiNodeQuery,
    reach: &VerificationPointerReach,
    dismiss_controls: &[&'static str],
) -> Vec<(&'static str, Entity, Vec2)> {
    dismiss_controls
        .iter()
        .flat_map(|control| {
            let mut matches = reach
                .nodes_named(control)
                .into_iter()
                .filter_map(|entity| {
                    nodes
                        .get(entity)
                        .ok()
                        .filter(|(_, computed, _, visibility, enabled)| {
                            visibility.get()
                                && computed.size() != Vec2::ZERO
                                && enabled.is_none_or(|enabled| enabled.0)
                        })
                        .map(|(_, _, transform, _, _)| (*control, entity, transform.translation))
                        .filter(|(_, entity, position)| !reach.is_clipped_at(*entity, *position))
                })
                .collect::<Vec<_>>();
            matches.sort_by(|(_, _, left), (_, _, right)| {
                left.y.total_cmp(&right.y).then(left.x.total_cmp(&right.x))
            });
            matches
        })
        .collect()
}

/// Aims a click at the first candidate. The others are tried in turn if a press
/// would not reach it, and the first is clicked if none would.
fn click_first_dismiss_control(
    progress: &mut VerificationDismissalProgress,
    mut candidates: Vec<(&'static str, Entity, Vec2)>,
    frame: u32,
    closes_options: bool,
) -> Option<(&'static str, VerificationDismissalWait)> {
    if candidates.is_empty() {
        return None;
    }
    let (control, node, position) = candidates.remove(0);
    progress.dismiss_alternatives = if candidates.is_empty() {
        Vec::new()
    } else {
        candidates
            .into_iter()
            .map(|(_, node, position)| (node, position))
            .chain(std::iter::once((node, position)))
            .collect()
    };
    Some((
        control,
        VerificationDismissalWait::ClickControl {
            node,
            position,
            // This state is first advanced on the next frame, when the pointer moves.
            started_frame: frame + 1,
            closes_options,
        },
    ))
}
