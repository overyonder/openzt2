use super::authored_ui_document_lowering::{lower_ui_document, AuthoredUiDocument};
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    ui::parser::SourceUiDocument,
};
use openzt2_game_data::{
    ui_document::{
        action::{
            construction::UiConstructionAction, economy::UiEconomyAction,
            presentation::UiPresentationAction, UiActionRecord, UiTrigger,
        },
        document::{UiDocument, UiDocumentRole},
    },
    AssetId,
};
use std::collections::BTreeMap;

fn lower_authored_test_document(
    path: &str,
    role: UiDocumentRole,
    source: &[u8],
    role_targets: BTreeMap<String, UiDocumentRole>,
) -> UiDocument {
    let parsed = parse_blue_fang_source_document(AssetPath::new(path), source).unwrap();
    lower_ui_document(&AuthoredUiDocument {
        virtual_path: path.into(),
        id: AssetId::default(),
        role,
        source: SourceUiDocument::parse(&parsed),
        templates: Default::default(),
        image_selections: Vec::new(),
        interaction_cursors: None,
        placement_preview: None,
        role_targets,
        resolved_dependencies: Default::default(),
    })
    .unwrap()
}

fn node_actions<'a>(document: &'a UiDocument, name: &str) -> &'a [UiActionRecord] {
    &document
        .nodes
        .iter()
        .find(|node| node.name == name)
        .unwrap()
        .actions
}

#[test]
fn sell_quotes_the_selection_and_opens_the_confirmation_dialog() {
    let document = lower_authored_test_document(
        "ui/layout/entityinfo.xml",
        UiDocumentRole::EntityInfo,
        br#"<UILayout name="entity info"><UIRegion w="1024" h="768"/><children>
            <UIButton name="Sell"><activate>
                <event msg="ZT_UPDATE_SELL_MESSAGE"/>
                <event msg="UI_CHILD" data="UIChildData" name="delete_object_confirmation">
                    <child msg="UI_SHOW"/>
                </event>
            </activate></UIButton>
        </children></UILayout>"#,
        [("delete_object_confirmation".into(), UiDocumentRole::Modal)].into(),
    );
    let actions = node_actions(&document, "Sell");
    assert!(actions.iter().any(|action| matches!(action,
        UiActionRecord::Economy(record)
            if record.trigger == UiTrigger::Press
                && record.action == UiEconomyAction::RefreshSelectedEntitySellQuote
    )));
    assert!(actions.iter().any(|action| matches!(action,
        UiActionRecord::Presentation(record)
            if record.trigger == UiTrigger::Press
                && record.action == UiPresentationAction::SetDocumentNodeVisible {
                    document_role: UiDocumentRole::Modal,
                    target_node: UiDocumentRole::Modal.node_id("delete_object_confirmation"),
                    visible: true,
                }
    )));
}

#[test]
fn answering_the_sale_confirmation_closes_the_confirmation_document() {
    let document = lower_authored_test_document(
        "ui/layout/confirm.xml",
        UiDocumentRole::Modal,
        br#"<UILayout name="Confirm Dialog"><UIRegion w="1024" h="768"/><children>
            <UILayout name="delete_object_confirmation" modal="true">
                <UIState active="normal" visible="hidden"/>
                <children>
                    <UIButton name="cancel"><activate>
                        <event msg="UI_CHILD" data="UIChildData" name="delete_object_confirmation">
                            <child msg="UI_HIDE"/>
                        </event>
                    </activate></UIButton>
                    <UIButton name="delete_object"><activate>
                        <event msg="ZT_SELL_ENTITY"/>
                        <event msg="UI_CHILD" data="UIChildData" name="delete_object_confirmation">
                            <child msg="UI_HIDE"/>
                        </event>
                    </activate></UIButton>
                    <UILayout name="nested" modal="true"/>
                </children>
            </UILayout>
        </children></UILayout>"#,
        BTreeMap::new(),
    );
    let closes_document =
        |actions: &[UiActionRecord]| {
            actions.iter().any(|action| matches!(action,
            UiActionRecord::Presentation(record)
                if record.trigger == UiTrigger::Hide
                    && record.action
                        == UiPresentationAction::HideOwningDocumentAfterConfirmationDismissal
        ))
        };
    assert!(closes_document(node_actions(
        &document,
        "delete_object_confirmation"
    )));
    assert!(!closes_document(node_actions(&document, "nested")));
    let hides_dialog = |actions: &[UiActionRecord]| {
        actions.iter().any(|action| matches!(action,
            UiActionRecord::Presentation(record)
                if record.trigger == UiTrigger::Press
                    && record.action == UiPresentationAction::SetTargetNodeVisible {
                        target_node: UiDocumentRole::Modal.node_id("delete_object_confirmation"),
                        visible: false,
                    }
        ))
    };
    let sells = |actions: &[UiActionRecord]| {
        actions
            .iter()
            .filter(|action| {
                matches!(action,
                    UiActionRecord::Construction(record)
                        if record.trigger == UiTrigger::Press
                            && record.action == UiConstructionAction::SellSelected
                )
            })
            .count()
    };
    let confirm = node_actions(&document, "delete_object");
    assert!(hides_dialog(confirm));
    assert_eq!(sells(confirm), 1);
    let cancel = node_actions(&document, "cancel");
    assert!(hides_dialog(cancel));
    assert_eq!(sells(cancel), 0);
}
