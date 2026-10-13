use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::plugins::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use super::adoption_and_content_availability_types::{
    MakeScenarioContentAvailableRequest, ScenarioContentAvailability,
};

pub(crate) fn apply_scenario_content_availability_requests(
    mut commands: Commands,
    mut requests: MessageReader<MakeScenarioContentAvailableRequest>,
    members: Query<&WorldMember>,
    mut roots: Query<Option<&mut ScenarioContentAvailability>, With<WorldRoot>>,
) {
    let mut definitions_by_root = BTreeMap::<Entity, Vec<_>>::new();
    for request in requests.read() {
        let root = members
            .get(request.source)
            .map_or(request.source, |member| member.root);
        if roots.contains(root) {
            definitions_by_root
                .entry(root)
                .or_default()
                .push(request.definition);
        }
    }
    for (root, definitions) in definitions_by_root {
        let Ok(availability) = roots.get_mut(root) else {
            continue;
        };
        if let Some(mut availability) = availability {
            for definition in definitions {
                availability.make_available(definition);
            }
        } else {
            let mut availability = ScenarioContentAvailability::default();
            for definition in definitions {
                availability.make_available(definition);
            }
            commands.entity(root).insert(availability);
        }
    }
}
