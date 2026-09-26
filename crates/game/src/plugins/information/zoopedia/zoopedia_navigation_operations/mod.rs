use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::{action::information::UiInformationAction, document::UiDocumentRole},
    AssetId,
};

use crate::plugins::ui::{
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_node_projection_components::UiDocumentRoot,
    ui_document_lifecycle_contracts::ShowUiRole,
};

use crate::plugins::information::zoopedia::zoopedia_navigation_types::{
    PendingZoopediaEntry, ZoopediaHistory, ZoopediaNavigationActionContext, ZoopediaPage,
};

pub(in crate::plugins::information) fn apply_authored_zoopedia_navigation_action(
    action: &UiInformationAction,
    context: &mut ZoopediaNavigationActionContext<'_, '_, '_>,
) {
    match action {
        UiInformationAction::OpenEncyclopediaEntry { entry } => {
            open_zoopedia_for_subject(AssetId(entry.0), context);
        }
        UiInformationAction::OpenContextEncyclopediaEntry => {
            let catalogue_definition = matches!(
                context.source_document_role,
                UiDocumentRole::PurchaseCatalogue
            )
            .then_some(context.selected_catalogue_entry.0)
            .flatten();
            let selected_definition = context
                .selected_world_entity
                .and_then(|entity| context.inspectable_world_entities.get(entity).ok())
                .map(|inspectable| inspectable.definition);

            let zoopedia_subject = catalogue_definition
                .or(selected_definition)
                .and_then(|definition| context.world_definitions?.find_object(definition))
                .map(|object| object.zoopedia_subject)
                .filter(|subject| *subject != AssetId::default());

            if let Some(subject) = zoopedia_subject {
                open_zoopedia_for_subject(subject, context);
            }
        }
        UiInformationAction::ZoopediaBack => {
            visit_adjacent_zoopedia_history_subject(
                context.source_document_entity,
                context.zoopedia_pages,
                ZoopediaHistory::visit_previous_subject,
            );
        }
        UiInformationAction::ZoopediaForward => {
            visit_adjacent_zoopedia_history_subject(
                context.source_document_entity,
                context.zoopedia_pages,
                ZoopediaHistory::visit_next_subject,
            );
        }
        _ => unreachable!("non-Zoopedia action sent to the Zoopedia navigation owner"),
    }
}

fn open_zoopedia_for_subject(
    subject: AssetId,
    context: &mut ZoopediaNavigationActionContext<'_, '_, '_>,
) {
    let source_lifecycle_owner_entity = context.source_lifecycle_owner_entity;
    context.commands.queue(move |world: &mut World| {
        world.write_message(ShowUiRole {
            role: UiDocumentRole::Zoopedia,
            owner: source_lifecycle_owner_entity,
        });
    });

    // The shell's general Zoopedia button is authored as an empty hyperlink.
    // It opens the hydrated home page; only a nonempty subject navigates.
    if subject == AssetId::default() {
        return;
    }

    let mut matching_zoopedia_page_was_found = false;
    for (page_entity, candidate_document_owner, mut page, history) in
        context.zoopedia_pages.iter_mut()
    {
        let belongs_to_source_lifecycle = context
            .document_roots
            .get(candidate_document_owner.0)
            .is_ok_and(|(_, parent)| parent.parent() == source_lifecycle_owner_entity);
        if !belongs_to_source_lifecycle {
            continue;
        }

        matching_zoopedia_page_was_found = true;
        let previous_subject = page.subject;
        page.subject = subject;
        page.section = 0;
        if let Some(mut history) = history {
            history.visit_subject(subject);
        } else {
            context
                .commands
                .entity(page_entity)
                .insert(ZoopediaHistory::from_transition(previous_subject, subject));
        }
    }

    if !matching_zoopedia_page_was_found {
        context
            .commands
            .entity(source_lifecycle_owner_entity)
            .insert(PendingZoopediaEntry(subject));
    }
}

fn visit_adjacent_zoopedia_history_subject(
    source_document_entity: Entity,
    zoopedia_pages: &mut Query<(
        Entity,
        &UiDocumentOwner,
        &mut ZoopediaPage,
        Option<&mut ZoopediaHistory>,
    )>,
    visit_history_subject: impl Fn(&mut ZoopediaHistory) -> Option<AssetId>,
) {
    for (_, candidate_document_owner, mut page, history) in zoopedia_pages.iter_mut() {
        if candidate_document_owner.0 != source_document_entity {
            continue;
        }
        let Some(mut history) = history else {
            continue;
        };
        let Some(subject) = visit_history_subject(&mut history) else {
            continue;
        };

        page.subject = subject;
        page.section = 0;
    }
}

pub(in crate::plugins::information) fn apply_pending_zoopedia_entries_to_hydrated_pages(
    mut commands: Commands,
    pending_entries: Query<(Entity, &PendingZoopediaEntry)>,
    document_root_parents: Query<&ChildOf, With<UiDocumentRoot>>,
    mut zoopedia_pages: Query<(
        Entity,
        &UiDocumentOwner,
        &mut ZoopediaPage,
        Option<&mut ZoopediaHistory>,
    )>,
) {
    for (lifecycle_owner_entity, pending_entry) in &pending_entries {
        if pending_entry.0 == AssetId::default() {
            commands
                .entity(lifecycle_owner_entity)
                .remove::<PendingZoopediaEntry>();
            continue;
        }

        let Some((page_entity, _, mut page, history)) =
            zoopedia_pages
                .iter_mut()
                .find(|(_, candidate_document_owner, _, _)| {
                    document_root_parents
                        .get(candidate_document_owner.0)
                        .is_ok_and(|parent| parent.parent() == lifecycle_owner_entity)
                })
        else {
            continue;
        };

        let previous_subject = page.subject;
        page.subject = pending_entry.0;
        page.section = 0;
        if let Some(mut history) = history {
            history.visit_subject(pending_entry.0);
        } else {
            commands
                .entity(page_entity)
                .insert(ZoopediaHistory::from_transition(
                    previous_subject,
                    pending_entry.0,
                ));
        }
        commands
            .entity(lifecycle_owner_entity)
            .remove::<PendingZoopediaEntry>();
    }
}
