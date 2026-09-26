use super::{
    guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
    money_types::Money,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiEconomyActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;
use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::economy::UiEconomyAction, node_property_binding::UiTextPropertyBindingSource,
};
use std::fmt::Write;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct AuthoredZooAdmissionPriceBands(pub [Money; 4]);

#[derive(Component)]
pub(super) struct WorldZooAdmissionPolicyInitialized;

/// Projects the canonical admission price into the shipped zoo-status money field.
pub(super) fn project_zoo_admission_price_into_authored_ui_text(
    admission_price: Res<AdmissionPrice>,
    mut fields: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    for (binding, mut text) in &mut fields {
        if !matches!(binding.0, UiTextPropertyBindingSource::ZooAdmissionPrice)
            || (!admission_price.is_changed() && !binding.is_added())
        {
            continue;
        }
        text.0.clear();
        let cents = admission_price.0 .0.unsigned_abs();
        let _ = write!(text.0, "${}.{:02}", cents / 100, cents % 100);
    }
}

pub(super) fn initialize_zoo_admission_price_bands_from_loaded_world_definition(
    roots: Query<
        Entity,
        (
            With<WorldRoot>,
            With<WorldLoadCompleted>,
            Without<WorldZooAdmissionPolicyInitialized>,
        ),
    >,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    mut commands: Commands,
) {
    let Some(root) = roots.iter().next() else {
        return;
    };
    if let Some(policy) = active_definitions
        .get(&definitions)
        .and_then(WorldDefinitionsView::admission_price_bands_cents)
    {
        commands.insert_resource(AuthoredZooAdmissionPriceBands(
            policy.map(|value| Money(i64::from(value))),
        ));
        commands
            .entity(root)
            .insert(WorldZooAdmissionPolicyInitialized);
    }
}

pub(super) fn apply_zoo_admission_policy_changes_from_authored_ui_actions(
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: Query<(&UiEconomyActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    price_bands: Res<AuthoredZooAdmissionPriceBands>,
    mut admission_price: ResMut<AdmissionPrice>,
    mut admissions_open: ResMut<ZooAdmissionsOpen>,
) {
    for activation in activations.read() {
        let Ok((action_records, document_owner)) = nodes.get(activation.node) else {
            continue;
        };
        let Ok(document_root) = roots.get(document_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&document_root.document) else {
            continue;
        };

        for record in action_records.authored_action_records(document) {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiEconomyAction::SetZooAdmissionPriceBand { price_band_index } => {
                    if let Some(price) = usize::try_from(*price_band_index)
                        .ok()
                        .and_then(|band_index| price_bands.0.get(band_index).copied())
                    {
                        admission_price.0 = price;
                    }
                }
                UiEconomyAction::SetZooAdmissionsOpen { open } => admissions_open.0 = *open,
                _ => {}
            }
        }
    }
}
