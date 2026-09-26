use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::AssetId;

use super::authored_reusable_list_and_table_runtime_types::{UiListPolicy, UiListRow};
use super::{
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_node_projection_components::UiNodeId,
};
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// The opener toggles the drop-list display. Choosing the display or a row closes it.
pub(super) fn toggle_or_close_authored_drop_list_displays_from_activations(
    mut activations: MessageReader<UiNodeActivated>,
    activated_nodes: Query<(&UiNodeId, &UiDocumentOwner)>,
    rows: Query<&UiListRow>,
    parents: Query<&ChildOf>,
    policies: Query<(Entity, &UiNodeId, &UiDocumentOwner, &UiListPolicy)>,
    mut nodes: Query<(&UiNodeId, &UiDocumentOwner, &mut Visibility)>,
) {
    for activation in activations.read() {
        if activation.trigger != UiTrigger::Press {
            continue;
        }
        let Ok((activated, owner)) = activated_nodes.get(activation.node) else {
            continue;
        };
        let row = std::iter::successors(Some(activation.node), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .find_map(|entity| rows.get(entity).ok());

        for (list_entity, list, list_owner, policy) in &policies {
            if policy.drop_list_display_node == AssetId::default() {
                continue;
            }
            let toggle = list_owner == owner
                && policy.opener_node != AssetId::default()
                && activated.id == policy.opener_node;
            let close = (list_owner == owner && activated.id == list.id)
                || row.is_some_and(|row| row.list == list_entity);
            if !toggle && !close {
                continue;
            }

            let Some(mut visibility) = nodes.iter_mut().find_map(|(id, node_owner, visibility)| {
                ((node_owner, id.id) == (list_owner, policy.drop_list_display_node))
                    .then_some(visibility)
            }) else {
                continue;
            };
            let next = if toggle && *visibility == Visibility::Hidden {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != next {
                *visibility = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::authored_ui_layout_participation::{
        synchronize_authored_visibility_with_bevy_layout_participation, UiAuthoredLayoutDisplay,
    };
    use super::*;
    use crate::plugins::input::input_types::ActionSource;

    #[test]
    fn drop_list_opens_authored_hidden_display_and_closes_from_nested_row_document() {
        let mut app = App::new();
        app.add_message::<UiNodeActivated>().add_systems(
            Update,
            (
                toggle_or_close_authored_drop_list_displays_from_activations,
                synchronize_authored_visibility_with_bevy_layout_participation,
            )
                .chain(),
        );
        let owner = UiDocumentOwner(app.world_mut().spawn_empty().id());
        let opener_id = AssetId::from_key("opener");
        let display_id = AssetId::from_key("display");
        let opener = app
            .world_mut()
            .spawn((
                owner,
                UiNodeId {
                    index: 0,
                    id: opener_id,
                },
            ))
            .id();
        let display = app
            .world_mut()
            .spawn((
                owner,
                UiNodeId {
                    index: 1,
                    id: display_id,
                },
                Node {
                    display: Display::None,
                    ..Node::default()
                },
                Visibility::Hidden,
                UiAuthoredLayoutDisplay::from_authored_display(Display::Flex),
            ))
            .id();
        let list = app
            .world_mut()
            .spawn((
                owner,
                UiNodeId {
                    index: 2,
                    id: AssetId::from_key("list"),
                },
                UiListPolicy {
                    row_document: None,
                    opener_node: opener_id,
                    drop_list_display_node: display_id,
                    source: Default::default(),
                },
            ))
            .id();
        let row_owner = UiDocumentOwner(app.world_mut().spawn_empty().id());
        let row = app
            .world_mut()
            .spawn((
                row_owner,
                UiNodeId {
                    index: 0,
                    id: AssetId::from_key("row"),
                },
                UiListRow { list, index: 0 },
            ))
            .id();
        app.world_mut().write_message(UiNodeActivated {
            source: ActionSource::System,
            node: opener,
            trigger: UiTrigger::Press,
        });
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(display),
            Some(&Visibility::Inherited)
        );
        assert!(app
            .world()
            .get::<Node>(display)
            .is_some_and(|node| node.display == Display::Flex));
        app.world_mut().write_message(UiNodeActivated {
            source: ActionSource::System,
            node: row,
            trigger: UiTrigger::Press,
        });
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(display),
            Some(&Visibility::Hidden)
        );
        assert!(app
            .world()
            .get::<Node>(display)
            .is_some_and(|node| node.display == Display::None));
    }
}
