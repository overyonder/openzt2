use super::{
    guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
    money_types::Money,
};
use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiEconomyActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;
use crate::plugins::ui::authored_ui_change_activation_dispatch::UiPreviousSelection;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_node_projection_components::UiNodeId;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;
use crate::plugins::ui::authored_ui_text_layout_presentation::UiPhysicalFontSize;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;
use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::{economy::UiEconomyAction, presentation::UiPresentationAction, UiActionRecord, UiTrigger},
    document::{UiDocument, UiDocumentRole},
    node_property_binding::UiTextPropertyBindingSource,
};
use openzt2_game_data::AssetId;
use std::fmt::Write;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct AuthoredZooAdmissionPriceBands(pub [Money; 4]);

#[derive(Component)]
pub(super) struct WorldZooAdmissionPolicyInitialized;

pub(super) fn project_zoo_admissions_state_into_authored_toggle_selection(
    admissions_open: Res<ZooAdmissionsOpen>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut toggles: Query<(
        Ref<UiEconomyActions>,
        &UiDocumentOwner,
        &mut UiSelected,
        &mut UiPreviousSelection,
    )>,
) {
    for (actions, owner, mut selected, mut previous) in &mut toggles {
        if !admissions_open.is_changed() && !documents.is_changed() && !actions.is_changed() {
            continue;
        }
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
            .filter(|document| document.canonical_ui_document().role == UiDocumentRole::ZooStatus)
        else {
            continue;
        };
        let open = actions
            .authored_action_records(document)
            .find_map(|record| {
                if record.trigger != UiTrigger::On {
                    return None;
                }
                match &record.action {
                    UiEconomyAction::SetZooAdmissionsOpen { open } => Some(*open),
                    _ => None,
                }
            });
        let Some(open) = open else {
            continue;
        };
        let next = admissions_open.0 == open;
        if selected.0 != next {
            selected.0 = next;
            // Presentation changes must not dispatch the button's policy action.
            previous.synchronize_with_non_authored_selection_change(next);
        }
    }
}

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

pub(super) fn project_zoo_admissions_status_into_authored_ui_text(
    admissions_open: Res<ZooAdmissionsOpen>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut fields: Query<(
        Entity,
        Ref<UiTextBinding>,
        &UiNodeId,
        &UiDocumentOwner,
        &mut Text,
        &mut TextColor,
        &mut TextFont,
        &mut UiPhysicalFontSize,
        Option<&mut TextShadow>,
    )>,
    mut commands: Commands,
) {
    let changed = admissions_open.is_changed()
        || documents.is_changed()
        || active_localization.is_changed()
        || localizations.is_changed();
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    for (entity, binding, id, owner, mut text, mut color, mut font, mut physical_size, shadow) in
        &mut fields
    {
        if !matches!(binding.0, UiTextPropertyBindingSource::ZooAdmissionsStatus)
            || (!changed && !binding.is_changed())
        {
            continue;
        }
        let document = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
            .map(UiDocumentAsset::canonical_ui_document)
            .filter(|document| document.role == UiDocumentRole::ZooStatus);
        let key = document.and_then(|document| {
            authored_zoo_admissions_status_localization_key(document, id.id, admissions_open.0)
        });
        let value = localization
            .zip(key)
            .and_then(|(localization, key)| localization.find_plain_localized_text(key));
        let value = value.unwrap_or_default();
        if text.0 != value {
            text.0.clear();
            text.0.push_str(value);
        }
        let presentation_key = document
            .and_then(|document| document.nodes.get(id.index as usize))
            .filter(|node| node.id == id.id && node.text_format != AssetId::default())
            .map(|node| node.text_format)
            .or(key);
        let Some(presentation) = localization
            .zip(presentation_key)
            .and_then(|(localization, key)| localization.find_localized_text_presentation(key))
        else {
            continue;
        };
        if let Some(rgba) = presentation.text_color_rgba {
            color.set_if_neq(TextColor(Color::srgba_u8(
                rgba[0], rgba[1], rgba[2], rgba[3],
            )));
        }
        if let Some(size) = presentation.font_size_pixels {
            if font.font_size != FontSize::Px(size) {
                font.font_size = FontSize::Px(size);
            }
            if physical_size.physical_pixels() != size {
                *physical_size = UiPhysicalFontSize::from_physical_pixels(size);
            }
        }
        if presentation.bold_text && font.weight != FontWeight::BOLD {
            font.weight = FontWeight::BOLD;
        }
        if let Some(offset) = presentation.shadow_offset_pixels {
            let offset = Vec2::new(f32::from(offset[0]), f32::from(offset[1]));
            if let Some(mut shadow) = shadow {
                if shadow.offset != offset {
                    shadow.offset = offset;
                }
                if let Some(alpha) = presentation.shadow_alpha {
                    let color = shadow.color.with_alpha(f32::from(alpha) / 255.0);
                    if shadow.color != color {
                        shadow.color = color;
                    }
                }
            } else {
                let mut shadow = TextShadow {
                    offset,
                    ..default()
                };
                if let Some(alpha) = presentation.shadow_alpha {
                    shadow.color = shadow.color.with_alpha(f32::from(alpha) / 255.0);
                }
                commands.entity(entity).insert(shadow);
            }
        }
    }
}

fn authored_zoo_admissions_status_localization_key(
    document: &UiDocument,
    target: AssetId,
    open: bool,
) -> Option<AssetId> {
    // The admission button pairs its policy action with the status field's
    // localization action. Borrow those keys rather than authoring a second map.
    document.nodes.iter().find_map(|node| {
        node.actions.iter().find_map(|action| {
            let UiActionRecord::Economy(policy) = action else {
                return None;
            };
            if policy.action != (UiEconomyAction::SetZooAdmissionsOpen { open }) {
                return None;
            }
            node.actions.iter().find_map(|action| {
                let UiActionRecord::Presentation(presentation) = action else {
                    return None;
                };
                match &presentation.action {
                    UiPresentationAction::SetTargetNodeTextFromLocalizationKey {
                        target_node,
                        localization_key,
                    } if presentation.trigger == policy.trigger && *target_node == target => {
                        Some(*localization_key)
                    }
                    _ => None,
                }
            })
        })
    })
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
