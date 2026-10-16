use bevy::prelude::*;
use openzt2_zoo_state::progression::profile_scenario_variables::{
    ProfileScenarioVariables, SetProfileScenarioVariable,
};

pub(super) fn apply_profile_scenario_variable_changes(
    mut requests: MessageReader<SetProfileScenarioVariable>,
    mut variables: ResMut<ProfileScenarioVariables>,
) {
    for request in requests.read() {
        if variables.get(request.key) == request.value.as_deref() {
            continue;
        }
        if let Err(reason) = variables.set(request.key, request.value.as_deref()) {
            error!(key = ?request.key, reason, "scenario profile variable could not be applied");
        }
    }
}
