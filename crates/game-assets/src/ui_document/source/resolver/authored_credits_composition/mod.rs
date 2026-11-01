//! Selected credits pages share the menu's authored timeline and canvas.

use std::collections::BTreeMap;

use crate::source_document::ui::{
    model::{SourceUiNode, SourceUiWidgetData},
    parser::SourceUiDocument,
};

pub(super) fn compose_authored_credits_fragments_into_timeline(
    documents: &mut BTreeMap<String, SourceUiDocument>,
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
