use super::super::original_profile_save_locations::OriginalProfileSaveLocations;
use bevy::{
    prelude::*,
    tasks::{block_on, poll_once, IoTaskPool, Task},
};
use openzt2_game_assets::original_saved_game_source::OriginalSavedGameSources;
use openzt2_game_data::AssetId;
use openzt2_interface_state::persistence::{
    profile_types::{ProfileOptions, ProfileSelected},
    save_slot_types::{LoadSaveSlotCatalogue, SaveSlotCatalogue, SaveSlotCatalogueReady},
    saved_game_source_types::OriginalSavedGameCatalogueFailed,
};

#[derive(Component)]
pub(in crate::persistence) struct OriginalSavedGameCatalogueTask {
    profile: AssetId,
    task: Task<
        Result<
            super::original_saved_game_catalogue_reading::OriginalSavedGameCatalogueRead,
            String,
        >,
    >,
}

pub(in crate::persistence) fn begin_original_saved_game_catalogue_loading(
    mut commands: Commands,
    mut requests: MessageReader<LoadSaveSlotCatalogue>,
    mut selected: MessageReader<ProfileSelected>,
    profile: Res<ProfileOptions>,
    locations: Res<OriginalProfileSaveLocations>,
    sources: Option<Res<OriginalSavedGameSources>>,
    tasks: Query<(), With<OriginalSavedGameCatalogueTask>>,
    mut failures: MessageWriter<OriginalSavedGameCatalogueFailed>,
) {
    let requested = requests.read().next().is_some() | selected.read().next().is_some();
    if !requested || !tasks.is_empty() {
        return;
    }
    let Some(sources) = sources.as_deref().cloned() else {
        failures.write(OriginalSavedGameCatalogueFailed {
            reason: "original saved-game source is unavailable".into(),
        });
        return;
    };
    let locations = (*locations).clone();
    commands.spawn(OriginalSavedGameCatalogueTask {
        profile: profile.profile_identifier,
        task: IoTaskPool::get().spawn(async move {
            super::original_saved_game_catalogue_reading::read_original_saved_game_catalogue(
                &locations, &sources,
            )
            .map_err(|error| error.to_string())
        }),
    });
}

pub(in crate::persistence) fn complete_original_saved_game_catalogue_loading(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut OriginalSavedGameCatalogueTask)>,
    profile: Res<ProfileOptions>,
    mut catalogue: ResMut<SaveSlotCatalogue>,
    mut ready: MessageWriter<SaveSlotCatalogueReady>,
    mut reload: MessageWriter<LoadSaveSlotCatalogue>,
    mut failures: MessageWriter<OriginalSavedGameCatalogueFailed>,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = block_on(poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if task.profile != profile.profile_identifier {
            reload.write(LoadSaveSlotCatalogue);
            continue;
        }
        match result {
            Ok(result) => {
                catalogue.original_saved_game_records = result.records;
                for reason in result.failures {
                    failures.write(OriginalSavedGameCatalogueFailed { reason });
                }
                ready.write(SaveSlotCatalogueReady);
            }
            Err(reason) => {
                failures.write(OriginalSavedGameCatalogueFailed { reason });
            }
        }
    }
}
