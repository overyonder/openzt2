use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_economy::economy::display_currency_cents;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::{
    document::UiDocumentRole, node_property_binding::UiIntegerPropertyBindingSource,
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentMembers, UiDocumentRoot, UiValue, UiValueBinding,
};
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldRoot,
};
use openzt2_zoo_state::donations::donation_payment_types::{DonationTotal, ZooDonationSummary};

use super::{
    show_schedule_types::{ScheduledShowRow, SelectedShowScheduleRow},
    show_stage_types::ShowStage,
};

pub(super) fn project_selected_or_all_show_donation_summary_into_authored_controls(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<(
        Entity,
        Ref<UiDocumentRoot>,
        Option<Ref<ChildOf>>,
        Ref<UiDocumentMembers>,
    )>,
    selections: Query<Ref<SelectedShowScheduleRow>>,
    scheduled: Query<Ref<ScheduledShowRow>>,
    stages: Query<Ref<DonationTotal>, With<ShowStage>>,
    zoo: Query<Ref<ZooDonationSummary>, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    mut removed: (
        RemovedComponents<SelectedShowScheduleRow>,
        RemovedComponents<DonationTotal>,
    ),
    mut values: Query<(Ref<UiValueBinding>, &mut UiValue)>,
) {
    let zoo = zoo.single().ok();
    let removed_selections: bevy::ecs::entity::EntityHashSet = removed.0.read().collect();
    let removed_totals: bevy::ecs::entity::EntityHashSet = removed.1.read().collect();
    for (root, document_root, parent, members) in &roots {
        let Some(document) = documents.get(&document_root.document) else {
            continue;
        };
        if document.canonical_ui_document().role != UiDocumentRole::ShowEditor {
            continue;
        }
        let controller = parent.as_ref().map_or(root, |parent| parent.parent());
        let selection = selections.get(controller).ok();
        let row = selection
            .as_ref()
            .and_then(|selection| scheduled.get(selection.0).ok());
        let stage = row.as_ref().and_then(|row| stages.get(row.stage).ok());
        let changed = documents.is_changed()
            || document_root.is_changed()
            || members.is_changed()
            || parent.as_ref().is_some_and(Ref::is_changed)
            || selection.as_ref().is_some_and(Ref::is_changed)
            || row.as_ref().is_some_and(Ref::is_changed)
            || removed_selections.contains(&controller)
            || row.as_ref().map_or_else(
                || zoo.as_ref().is_some_and(Ref::is_changed),
                |row| {
                    stage.as_ref().is_some_and(Ref::is_changed)
                        || removed_totals.contains(&row.stage)
                },
            );
        let aggregate = if row.is_some() {
            stage.as_deref().copied().unwrap_or_default()
        } else if let Some(total) = zoo
            .as_ref()
            .and_then(|zoo| zoo.total(Some(openzt2_game_data::AssetId::from_key("show-donation"))))
        {
            total
        } else {
            continue;
        };
        let mut fields = values.iter_many_mut(members.iter());
        while let Some(field) = fields.fetch_next() {
            let Ok((binding, mut value)) = field else {
                continue;
            };
            if !changed && !binding.is_changed() && !value.is_added() {
                continue;
            }
            let next = match binding.0 {
                UiIntegerPropertyBindingSource::DonationCount => i64::from(aggregate.count),
                UiIntegerPropertyBindingSource::DonationTotalCents => {
                    display_currency_cents(aggregate.amount)
                }
                UiIntegerPropertyBindingSource::DonationAverageCents => display_currency_cents(
                    aggregate
                        .amount
                        .checked_scale((aggregate.count as f32).recip())
                        .unwrap_or_default(),
                ),
                _ => continue,
            };
            value.set_if_neq(UiValue(next));
        }
    }
}
