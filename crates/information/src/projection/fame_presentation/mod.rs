use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::{
    ui_document::{document::UiDocumentRole, widget::UiWidgetRecord},
    AssetId,
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentOwner, UiDocumentRoot, UiNodeId, UiValue,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::progression::fame_types::FameAuthoredPresentation;

#[derive(Component, PartialEq, Eq)]
pub(crate) enum AuthoredFameField {
    Bar(AssetId),
    GateIcon,
}

pub(crate) fn bind_authored_fame_fields(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    mut nodes: Query<(
        Entity,
        Ref<UiNodeId>,
        &Name,
        &UiDocumentOwner,
        Option<&mut AuthoredFameField>,
    )>,
    mut commands: Commands,
) {
    for (entity, node, name, owner, bound) in &mut nodes {
        if !node.is_changed() && !documents.is_changed() {
            continue;
        }
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        if document.canonical_ui_document().role != UiDocumentRole::ZooStatus {
            continue;
        }
        let next = if name.as_str().eq_ignore_ascii_case("Zoo Gate") {
            document
                .canonical_ui_document()
                .nodes
                .get(node.index as usize)
                .filter(|node| matches!(node.widget, UiWidgetRecord::MultiIcon { .. }))
                .map(|_| AuthoredFameField::GateIcon)
        } else if name.as_str().eq_ignore_ascii_case("bar") {
            parents
                .get(entity)
                .ok()
                .and_then(|parent| {
                    let group = parent.parent();
                    let layout = parents.get(group).ok()?.parent();
                    names
                        .get(layout)
                        .ok()?
                        .as_str()
                        .eq_ignore_ascii_case("zoo fame tab layout")
                        .then(|| names.get(group).ok())
                        .flatten()
                })
                .map(|name| {
                    AuthoredFameField::Bar(AssetId::from_key(&name.as_str().to_ascii_lowercase()))
                })
        } else {
            None
        };
        match (next, bound) {
            (Some(next), Some(mut current)) => {
                current.set_if_neq(next);
            }
            (Some(next), None) => {
                commands.entity(entity).insert(next);
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<AuthoredFameField>();
            }
            (None, None) => {}
        }
    }
}

pub(crate) fn project_fame_values_into_authored_fields(
    worlds: Query<Ref<FameAuthoredPresentation>, With<WorldRoot>>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut fields: Query<(
        Ref<AuthoredFameField>,
        &UiNodeId,
        &UiDocumentOwner,
        &mut Node,
        Option<&mut UiValue>,
    )>,
) {
    let Some(presentation) = worlds.iter().next() else {
        return;
    };
    for (field, node_id, owner, node, value) in &mut fields {
        if !presentation.is_changed() && !field.is_changed() && !documents.is_changed() {
            continue;
        }
        match *field {
            AuthoredFameField::Bar(category) => {
                if let Some(&width) = presentation.bars.get(&category) {
                    node.map_unchanged(|node| &mut node.width)
                        .set_if_neq(Val::Px(width));
                }
            }
            AuthoredFameField::GateIcon => {
                let (Some(icon), Some(mut value)) = (presentation.gate_icon, value) else {
                    continue;
                };
                let Some(document) = roots
                    .get(owner.0)
                    .ok()
                    .and_then(|root| documents.get(&root.document))
                else {
                    continue;
                };
                let Some(record) = document
                    .canonical_ui_document()
                    .nodes
                    .get(node_id.index as usize)
                else {
                    continue;
                };
                if let UiWidgetRecord::MultiIcon { entries } = &record.widget {
                    if let Some(index) = entries.iter().position(|entry| entry.key == icon) {
                        value.set_if_neq(UiValue(index as i64));
                    }
                }
            }
        }
    }
}
