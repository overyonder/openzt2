//! `ZT_PHOTOEVENT_SET_CURRENT_CHALLENGE`: the scenario's current photo
//! challenge set, whose members its photo goals count.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::photos::photo_challenge_types::{CurrentPhotoChallengeSet, PhotoChallengeSet};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SelectCurrentPhotoChallengeSetRequest {
    pub(super) set: AssetId,
}

pub(super) fn select_requested_current_photo_challenge_sets(
    mut commands: Commands,
    mut requests: MessageReader<SelectCurrentPhotoChallengeSetRequest>,
    sets: Query<(Entity, &PhotoChallengeSet, Has<CurrentPhotoChallengeSet>)>,
) {
    let Some(request) = requests.read().last() else {
        return;
    };
    if !sets.iter().any(|(_, set, _)| set.id == request.set) {
        error!(set = ?request.set, "scenario selected an unknown photo challenge set");
        return;
    }
    for (entity, set, current) in &sets {
        if set.id == request.set {
            commands.entity(entity).insert(CurrentPhotoChallengeSet);
        } else if current {
            commands.entity(entity).remove::<CurrentPhotoChallengeSet>();
        }
    }
}
