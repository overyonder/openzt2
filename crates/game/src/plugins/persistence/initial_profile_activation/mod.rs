use bevy::prelude::{Assets, Local, MessageReader, MessageWriter, Res};
use openzt2_game_data::AssetId;

use crate::assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};

use super::profile_types::{
    CreateProfile, LoadProfileIndex, ProfileCreated, ProfileIndex, ProfileIndexReady, SelectProfile,
};

pub(super) fn request_initial_profile_index_from_persistence_startup(
    mut profile_index_load_requests: MessageWriter<LoadProfileIndex>,
) {
    profile_index_load_requests.write(LoadProfileIndex);
}

/// Selects the saved profile, or creates the localized first-run profile.
pub(super) fn select_persisted_profile_or_create_localized_first_profile(
    mut profile_index_ready_messages: MessageReader<ProfileIndexReady>,
    mut profile_created_messages: MessageReader<ProfileCreated>,
    profile_index: Res<ProfileIndex>,
    active_localization_assets: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    mut profile_activation_pending: Local<bool>,
    mut profile_creation_requests: MessageWriter<CreateProfile>,
    mut profile_selection_requests: MessageWriter<SelectProfile>,
) {
    *profile_activation_pending |= profile_index_ready_messages.read().next().is_some();
    if *profile_activation_pending {
        if let Some(profile_identifier) = profile_index.selected_profile_identifier.or_else(|| {
            profile_index
                .profile_records
                .first()
                .map(|profile_record| profile_record.profile_identifier)
        }) {
            profile_selection_requests.write(SelectProfile {
                requested_profile_identifier: profile_identifier,
            });
            *profile_activation_pending = false;
        } else if let Some(localized_default_profile_display_name) = active_localization_assets
            .borrow_loaded_localization_view(&localization_assets)
            .and_then(|localization_asset| {
                localization_asset
                    .find_plain_localized_text(AssetId::from_key("mainmenu:profile_default"))
            })
        {
            profile_creation_requests.write(CreateProfile {
                requested_profile_display_name: localized_default_profile_display_name.to_owned(),
            });
            *profile_activation_pending = false;
        }
    }
    for profile_created_message in profile_created_messages.read() {
        profile_selection_requests.write(SelectProfile {
            requested_profile_identifier: profile_created_message.created_profile_identifier,
        });
    }
}
