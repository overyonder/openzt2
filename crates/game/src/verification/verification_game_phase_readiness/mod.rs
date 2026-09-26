use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::{
    application_lifecycle::GamePhase,
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        camera::camera_runtime_state_types::ZooCamera,
        shell::shell_selection_types::WorldChoiceView,
        terrain::terrain_chunk_presentation_types::TerrainRenderChunk,
        ui::authored_ui_node_projection_components::UiDocumentRoot,
        world_spawn::world_load_completion_marker::WorldLoadCompleted,
    },
};

/// A phase is ready once a player could act in it, not when the state first changes.
#[derive(SystemParam)]
pub(crate) struct VerificationGamePhaseReadiness<'w, 's> {
    phase: Res<'w, State<GamePhase>>,
    roots: Query<'w, 's, (&'static InheritedVisibility, &'static UiDocumentRoot)>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    world_choices: Query<'w, 's, (), With<WorldChoiceView>>,
    meshes: Query<'w, 's, (), With<Mesh3d>>,
    zoo_cameras: Query<'w, 's, (), With<ZooCamera>>,
    terrain_chunks: Query<'w, 's, (), With<TerrainRenderChunk>>,
    loaded_worlds: Query<'w, 's, (), With<WorldLoadCompleted>>,
}

impl VerificationGamePhaseReadiness<'_, '_> {
    pub(crate) fn current_phase(&self) -> GamePhase {
        *self.phase.get()
    }

    pub(crate) fn phase_is_ready_for_input(&self, phase: GamePhase) -> bool {
        if self.current_phase() != phase {
            return false;
        }
        let any_root_visible = self.roots.iter().any(|(visibility, _)| visibility.get());
        match phase {
            GamePhase::Boot | GamePhase::MainMenu | GamePhase::Loading => any_root_visible,
            GamePhase::MapSelection => {
                any_root_visible && !self.world_choices.is_empty() && !self.meshes.is_empty()
            }
            GamePhase::InGame => {
                !self.zoo_cameras.is_empty()
                    && !self.terrain_chunks.is_empty()
                    && !self.loaded_worlds.is_empty()
                    && self.roots.iter().any(|(visibility, root)| {
                        visibility.get()
                            && self.documents.get(&root.document).is_some_and(|document| {
                                document.canonical_ui_document().role == UiDocumentRole::InGameHud
                            })
                    })
            }
        }
    }
}

pub(crate) const fn game_phase_order(phase: GamePhase) -> u8 {
    match phase {
        GamePhase::Boot => 0,
        GamePhase::MainMenu => 1,
        GamePhase::MapSelection => 2,
        GamePhase::Loading => 3,
        GamePhase::InGame => 4,
    }
}
