use bevy::prelude::*;
use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::persistence::profile_types::{
    CreateProfile, LoadProfileIndex, ProfileCreated, ProfileIndex, ProfileIndexReady,
    ProfileOperationFailed, SelectProfile,
};

#[derive(Component)]
pub(super) struct AwaitingStartupProfileIndex;

#[derive(Component)]
pub(super) struct AwaitingLocalizedDefaultProfileName;

pub(super) fn begin_startup_profile_activation(
    mut commands: Commands,
    mut load_index: MessageWriter<LoadProfileIndex>,
) {
    commands.spawn(AwaitingStartupProfileIndex);
    load_index.write(LoadProfileIndex);
}

pub(super) fn activate_initial_profile_from_loaded_index(
    mut commands: Commands,
    pending: Query<Entity, With<AwaitingStartupProfileIndex>>,
    mut ready: MessageReader<ProfileIndexReady>,
    mut failures: MessageReader<ProfileOperationFailed>,
    index: Res<ProfileIndex>,
    mut select: MessageWriter<SelectProfile>,
) {
    let index_ready = !ready.is_empty();
    ready.clear();
    let index_failed = failures
        .read()
        .any(|failure| failure.affected_profile_identifier.is_none());
    failures.clear();
    let Ok(operation) = pending.single() else {
        return;
    };
    if index_ready {
        if let Some(profile) = index.selected_profile_identifier.or_else(|| {
            index
                .profile_records
                .first()
                .map(|record| record.profile_identifier)
        }) {
            select.write(SelectProfile {
                requested_profile_identifier: profile,
            });
            commands.entity(operation).despawn();
        } else {
            commands
                .entity(operation)
                .remove::<AwaitingStartupProfileIndex>()
                .insert(AwaitingLocalizedDefaultProfileName);
        }
    } else if index_failed {
        commands.entity(operation).despawn();
    }
}

pub(super) fn create_initial_profile_after_default_name_loads(
    mut commands: Commands,
    pending: Query<Entity, With<AwaitingLocalizedDefaultProfileName>>,
    localization: Res<LocalizationPrecedenceIndex>,
    assets: Res<Assets<LocalizationAsset>>,
    mut create: MessageWriter<CreateProfile>,
) {
    let Ok(operation) = pending.single() else {
        return;
    };
    let Some(display_name) = localization
        .borrow_loaded_localization_view(&assets)
        .and_then(|view| {
            view.find_plain_localized_text(AssetId::from_key("mainmenu:profile_default"))
        })
    else {
        return;
    };
    create.write(CreateProfile {
        requested_profile_display_name: display_name.to_owned(),
    });
    commands.entity(operation).despawn();
}

pub(super) fn select_created_profiles(
    mut created: MessageReader<ProfileCreated>,
    mut select: MessageWriter<SelectProfile>,
) {
    for created in created.read() {
        select.write(SelectProfile {
            requested_profile_identifier: created.created_profile_identifier,
        });
    }
}
