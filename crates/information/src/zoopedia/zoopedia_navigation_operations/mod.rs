use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::{action::information::UiInformationAction, document::UiDocumentRole},
    AssetId,
};

use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;

use super::zoopedia_navigation_types::{
    PendingZoopediaEntry, ZoopediaHistory, ZoopediaNavigationActionContext, ZoopediaPage,
};

pub(crate) fn apply_authored_zoopedia_navigation_action(
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
            let subject = catalogue_definition
                .or(selected_definition)
                .and_then(|definition| context.world_definitions?.find_object(definition))
                .map(|object| object.zoopedia_subject)
                .filter(|subject| *subject != AssetId::default());
            if let Some(subject) = subject {
                open_zoopedia_for_subject(subject, context);
            }
        }
        UiInformationAction::ZoopediaBack | UiInformationAction::ZoopediaForward => {
            let Ok((mut page, mut history)) = context
                .zoopedia_pages
                .get_mut(context.source_document_entity)
            else {
                return;
            };
            let subject = if matches!(action, UiInformationAction::ZoopediaBack) {
                ZoopediaHistory::visit_previous_subject(&mut history)
            } else {
                ZoopediaHistory::visit_next_subject(&mut history)
            };
            if let Some(subject) = subject {
                page.set_if_neq(ZoopediaPage {
                    subject,
                    section: 0,
                });
            }
        }
        _ => unreachable!("non-Zoopedia action sent to the Zoopedia navigation owner"),
    }
}

fn open_zoopedia_for_subject(
    subject: AssetId,
    context: &mut ZoopediaNavigationActionContext<'_, '_, '_>,
) {
    let owner = context.source_lifecycle_owner_entity;
    context.show_roles.write(ShowUiRole {
        role: UiDocumentRole::Zoopedia,
        owner,
    });

    // An empty authored hyperlink opens the current page without navigating.
    if subject == AssetId::default() {
        return;
    }
    let document = context
        .document_children
        .get(owner)
        .ok()
        .and_then(|children| {
            <Children as bevy::ecs::relationship::RelationshipTarget>::iter(children)
                .find(|child| context.zoopedia_pages.contains(*child))
        });
    if let Some(document) = document {
        if let Ok((mut page, mut history)) = context.zoopedia_pages.get_mut(document) {
            page.set_if_neq(ZoopediaPage {
                subject,
                section: 0,
            });
            history.visit_subject(subject);
        }
    } else {
        let entry = PendingZoopediaEntry(subject);
        if let Ok(mut pending) = context.pending.get_mut(owner) {
            pending.set_if_neq(entry);
        } else {
            context.commands.entity(owner).insert(entry);
        }
    }
}
