//! Selected credits pages share the menu's authored timeline and canvas.

use std::collections::BTreeMap;

use crate::source_document::ordered_source_document_types::OrderedSourceDocument;
use crate::source_document::ui::{
    model::{SourceUiNode, SourceUiWidgetData},
    parser::SourceUiDocument,
};

pub(super) fn compose_authored_credits_fragments_into_timeline(
    documents: &mut BTreeMap<String, SourceUiDocument>,
    selected_sources: &[&OrderedSourceDocument],
) {
    fn find_timeline(node: &mut SourceUiNode) -> Option<&mut SourceUiNode> {
        if node.name.as_deref() == Some("credits") {
            return Some(node);
        }
        node.children.iter_mut().find_map(find_timeline)
    }

    let fragment_paths = documents
        .keys()
        .filter(|path| path.starts_with("ui/layout/credits/") && path.ends_with(".xml"))
        .rev()
        .cloned()
        .collect::<Vec<_>>();
    let Some(mut shell) = documents.remove("ui/layout/credits.xml") else {
        return;
    };
    if let Some(timeline) = find_timeline(&mut shell.root) {
        for path in fragment_paths {
            let Some(fragment) = documents.remove(&path) else {
                continue;
            };
            if let Some(region) = fragment.root.region {
                timeline.region = Some(region);
            }
            if let Some(aspect) = fragment.root.aspect {
                timeline.aspect = Some(aspect);
            }
            if selected_sources
                .iter()
                .rev()
                .find(|source| source.path.key() == path)
                .is_some_and(|source| {
                    source
                        .root
                        .element_children()
                        .any(|node| node.name == "UIState")
                })
            {
                timeline.state = fragment.root.state;
            }
            timeline.children.extend(fragment.root.children);
            if let (
                SourceUiWidgetData::TimedEvents(destination),
                SourceUiWidgetData::TimedEvents(source),
            ) = (&mut timeline.widget, fragment.root.widget)
            {
                destination.extend(source);
            }
        }
    }
    documents.insert("ui/layout/credits.xml".into(), shell);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };
    use crate::ui_document::source::lower::authored_ui_document_lowering::{
        AuthoredUiDocument, lower_ui_document,
    };
    use openzt2_game_data::{
        AssetId,
        ui_document::{
            action::presentation::UiPresentationAction, document::UiDocumentRole,
            node::UiNodeFlags, node_layout::UiNodeRegionMetric, widget::UiWidgetRecord,
        },
    };

    #[test]
    fn credits_fragments_supply_the_canvas_and_cumulative_camera_and_card_events()
    -> Result<(), Box<dyn std::error::Error>> {
        let shell_path = "ui/layout/credits.xml";
        let page_path = "ui/layout/credits/credits_4.xml";
        let repeat_path = "ui/layout/credits/credits_00.xml";
        let sources = [
            parse_blue_fang_source_document(AssetPath::new(shell_path), br#"
                <UILayout><UIRegion w="1024" h="768"/><children>
                  <UILayout name="Credits Container"><children>
                    <UILayout name="railcam"/>
                    <ZTUITimedEvents name="credits">
                      <UIState active="disabled" visible="hidden"/>
                      <timedEvents/>
                    </ZTUITimedEvents>
                  </children></UILayout>
                </children></UILayout>"#)?,
            parse_blue_fang_source_document(AssetPath::new(page_path), br#"
                <ZTUITimedEvents name="credits">
                  <UIRegion x="0" y="0" w="1024" h="768" xalign="min" yalign="min" walign="len" halign="len"/>
                  <UIState active="normal" visible="normal"/>
                  <UIAspect string="credits canvas"><default/></UIAspect>
                  <children>
                    <UILayout name="background"><UIRegion w="1024" h="768"/></UILayout>
                    <UILayout name="card"><UIRegion w="1024" h="768"/><UIState visible="hidden"/></UILayout>
                  </children>
                  <timedEvents>
                    <event time="0" msg="UI_CHILD" name="railcam"><child msg="UI_ACTIVATE_OFF"/></event>
                    <event time="0" msg="UI_CHILD" name="background"><child msg="UI_SHOW"/></event>
                    <event time="2" msg="UI_CHILD" name="card"><child msg="UI_SHOW"/></event>
                    <event time="1" msg="UI_CHILD" name="card"><child msg="UI_HIDE"/></event>
                  </timedEvents>
                </ZTUITimedEvents>"#)?,
            parse_blue_fang_source_document(AssetPath::new(repeat_path), br#"
                <ZTUITimedEvents name="credits"><timedEvents>
                  <event time=".5" msg="UI_ACTIVATE_ON"/>
                </timedEvents></ZTUITimedEvents>"#)?,
        ];
        let mut documents = sources
            .iter()
            .map(|source| (source.path.key(), SourceUiDocument::parse(source)))
            .collect();
        compose_authored_credits_fragments_into_timeline(
            &mut documents,
            &sources.iter().collect::<Vec<_>>(),
        );
        let shell = documents
            .remove(shell_path)
            .ok_or("credits shell missing")?;
        let timeline = &shell.root.children[0].children[1];
        assert_eq!(
            timeline
                .aspect
                .as_ref()
                .and_then(|aspect| aspect.authored_string.as_deref()),
            Some("credits canvas")
        );
        assert_eq!(documents.len(), 0);
        let canonical = lower_ui_document(&AuthoredUiDocument {
            virtual_path: shell_path.into(),
            id: AssetId::default(),
            role: UiDocumentRole::MainMenu,
            source: shell,
            templates: BTreeMap::new(),
            image_selections: Vec::new(),
            interaction_cursors: None,
            placement_preview: None,
            role_targets: BTreeMap::new(),
            resolved_dependencies: Default::default(),
        })?;
        let timeline = canonical
            .nodes
            .iter()
            .find(|node| node.name == "credits")
            .ok_or("canonical timeline missing")?;
        assert_eq!(
            timeline.region.metrics[2..],
            [
                UiNodeRegionMetric::Pixels(1024.0),
                UiNodeRegionMetric::Pixels(768.0)
            ]
        );
        assert_ne!(timeline.flags.0 & UiNodeFlags::VISIBLE.0, 0);
        assert_ne!(timeline.flags.0 & UiNodeFlags::ENABLED.0, 0);
        let UiWidgetRecord::TimedSequence { events } = &timeline.widget else {
            return Err("credits did not retain its timed action sequence".into());
        };
        assert_eq!(
            events.iter().map(|event| event.due_ms).collect::<Vec<_>>(),
            [0, 0, 2000, 3000, 3500]
        );
        assert!(matches!(events[0].action.action,
            UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive { target_node, active: false, .. }
            if target_node == UiDocumentRole::node_id(UiDocumentRole::MainMenu, "railcam")));
        assert!(matches!(events[1].action.action,
            UiPresentationAction::SetTargetNodeVisible { target_node, visible: true }
            if target_node == UiDocumentRole::node_id(UiDocumentRole::MainMenu, "background")));
        assert!(matches!(events[4].action.action,
            UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive { target_node, active: true, .. }
            if target_node == timeline.id));
        Ok(())
    }
}
