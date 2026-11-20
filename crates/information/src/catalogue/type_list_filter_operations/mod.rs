use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::ui_document::action::information::UiInformationAction;

use openzt2_interface_state::information::catalogue_types::TypeListFilter;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentMembers, UiNodeId,
};

pub(crate) fn apply_authored_catalogue_type_list_filter_action(
    action: &UiInformationAction,
    source_document: Entity,
    members: &Query<&UiDocumentMembers>,
    mut_filters: &mut Query<(&UiNodeId, &mut TypeListFilter)>,
) {
    let target = match action {
        UiInformationAction::SetResearchFilter { list, .. }
        | UiInformationAction::SetCatalogueKindFilter { list, .. }
        | UiInformationAction::ClearTypeFilter { list } => *list,
        _ => unreachable!("non-filter action sent to the catalogue type-list filter owner"),
    };
    let Ok(members) = members.get(source_document) else {
        return;
    };
    for entity in members.iter() {
        let Ok((id, mut filter)) = mut_filters.get_mut(entity) else {
            continue;
        };
        if id.id != target {
            continue;
        }
        let mut next = *filter;
        match action {
            UiInformationAction::SetResearchFilter { unlocked_only, .. } => {
                next.unlocked_only = *unlocked_only
            }
            UiInformationAction::SetCatalogueKindFilter { kind, .. } => next.kind = Some(*kind),
            UiInformationAction::ClearTypeFilter { .. } => next = TypeListFilter::default(),
            _ => unreachable!(),
        }
        filter.set_if_neq(next);
    }
}
