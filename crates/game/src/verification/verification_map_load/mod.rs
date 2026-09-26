//! Loads one catalogue map through the production loaders and exits with the result.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::application_lifecycle::GamePhase;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::world_spawn::world_hydration_completion::WorldLoadFinished;
use crate::plugins::world_spawn::world_load_failure::WorldLoadFailed;
use crate::plugins::world_spawn::world_load_request_acceptance::BeginWorldLoad;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

pub(crate) struct VerificationMapLoadPlugin {
    pub(crate) map_index: usize,
}

impl Plugin for VerificationMapLoadPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MapLoadVerification {
            index: self.map_index,
            started: Instant::now(),
            selected: None,
            completed: None,
            exiting: false,
        })
        .add_systems(Last, verify_selected_map_load);
    }
}

#[derive(Resource)]
struct MapLoadVerification {
    index: usize,
    started: Instant,
    selected: Option<(AssetId, Instant)>,
    completed: Option<Instant>,
    exiting: bool,
}

fn verify_selected_map_load(
    mut verification: ResMut<MapLoadVerification>,
    active: Res<WorldScenarios>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    server: Res<AssetServer>,
    roots: Query<&WorldRoot>,
    pending_joint_attachments: Query<(Entity, &crate::plugins::animation_playback::model_joint_attachment_binding::PendingModelJointAttachment)>,
    children: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    models: Query<(
        &crate::plugins::world_spawn::prefab_presentation_types::PrefabModel,
        &InheritedVisibility,
    )>,
    mut requests: MessageWriter<BeginWorldLoad>,
    mut finished: MessageReader<WorldLoadFinished>,
    mut failed: MessageReader<WorldLoadFailed>,
    mut phase: ResMut<NextState<GamePhase>>,
    mut exit: MessageWriter<AppExit>,
) {
    if verification.exiting {
        return;
    }
    if verification.started.elapsed() > Duration::from_secs(120) {
        error!(target: "openzt2_map_load_suite", "map_load_result=timeout");
        verification.exiting = true;
        exit.write(AppExit::error());
        return;
    }
    let Some((selected, started)) = verification.selected else {
        if !active.catalogue_documents_are_loaded(&scenarios) {
            return;
        }
        let Some(catalogue) = active.get(&scenarios) else {
            return;
        };
        let count = catalogue.maps().count();
        if count == 0 {
            return;
        }
        info!(target: "openzt2_map_load_suite", map_count = count, "map catalogue ready");
        let Some(map) = catalogue.maps().nth(verification.index) else {
            error!(target: "openzt2_map_load_suite", "map_load_result=invalid_index");
            verification.exiting = true;
            exit.write(AppExit::error());
            return;
        };
        info!(target: "openzt2_map_load_suite", map = ?map.id,
            path = ?catalogue.handle_for_map(map.id).and_then(|handle| server.get_path(handle.id())),
            "map load requested");
        verification.selected = Some((map.id, Instant::now()));
        phase.set(GamePhase::Loading);
        requests.write(BeginWorldLoad {
            scenario: map.id,
            mode: WorldSessionMode::Freeform,
            profile: AssetId::default(),
            starting_cash_cents: None,
        });
        return;
    };
    for failure in failed.read() {
        if failure.scenario == selected {
            error!(target: "openzt2_map_load_suite", ?failure, "map_load_result=failed");
            verification.exiting = true;
            exit.write(AppExit::error());
            return;
        }
    }
    for result in finished.read() {
        if result.scenario == selected {
            verification.completed = Some(Instant::now());
            phase.set(GamePhase::InGame);
        }
    }
    if verification
        .completed
        .is_some_and(|at| at.elapsed() >= Duration::from_secs(2))
        && (pending_joint_attachments.is_empty() || started.elapsed() >= Duration::from_secs(10))
    {
        // Load time is reported, not judged: parallel suite runs share the machine.
        let successful = roots.iter().any(|root| root.scenario == selected)
            && pending_joint_attachments.is_empty();
        info!(target: "openzt2_map_load_suite", successful,
            load_seconds = verification
                .completed
                .map_or(0.0, |completed| (completed - started).as_secs_f64()),
            unresolved_joint_attachments = pending_joint_attachments.iter().count(),
            "map_load_result=completed");
        for (attachment, binding) in &pending_joint_attachments {
            binding.report_unresolved_binding(
                attachment, &children, &names, &parents, &models, &server,
            );
        }
        verification.exiting = true;
        exit.write(if successful {
            AppExit::Success
        } else {
            AppExit::error()
        });
    }
}
