//! Collection and transitive selection of authored UI templates.

use std::collections::{BTreeMap, BTreeSet};

use crate::assets::source_document::ui::{
    model::{SourceUiNode, SourceUiWidgetData},
    parser::SourceUiDocument,
};

use super::super::ui_source_document_gap::{
    UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
};

pub(super) fn select_transitively_referenced_ui_source_templates(
    root: &SourceUiNode,
    all_templates: &BTreeMap<String, SourceUiNode>,
) -> Result<BTreeMap<String, SourceUiNode>, UiSourceDocumentGap> {
    fn visit_referenced_ui_source_templates(
        node: &SourceUiNode,
        all_templates: &BTreeMap<String, SourceUiNode>,
        selected_templates: &mut BTreeMap<String, SourceUiNode>,
        visiting_template_names: &mut Vec<String>,
    ) -> Result<(), UiSourceDocumentGap> {
        if let Some(name) = &node.template {
            let key = name.to_ascii_lowercase();
            if visiting_template_names.contains(&key) {
                return Err(UiSourceDocumentGap {
                    family: UiSourceDocumentFamily::Ui,
                    kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                    virtual_path: "ui/template-resolution".into(),
                    span: node.span,
                    message: format!("UI template cycle contains {name:?}"),
                });
            }
            if !selected_templates.contains_key(&key) {
                let Some(template) = all_templates.get(&key) else {
                    return Ok(());
                };
                visiting_template_names.push(key.clone());
                visit_referenced_ui_source_templates(
                    template,
                    all_templates,
                    selected_templates,
                    visiting_template_names,
                )?;
                visiting_template_names.pop();
                selected_templates.insert(key, template.clone());
            }
        }
        node.children.iter().try_for_each(|child| {
            visit_referenced_ui_source_templates(
                child,
                all_templates,
                selected_templates,
                visiting_template_names,
            )
        })
    }

    let mut selected_templates = BTreeMap::new();
    visit_referenced_ui_source_templates(
        root,
        all_templates,
        &mut selected_templates,
        &mut Vec::new(),
    )?;
    Ok(selected_templates)
}

pub(super) fn collect_named_ui_source_templates(
    node: &SourceUiNode,
    templates: &mut BTreeMap<String, SourceUiNode>,
) -> Result<(), UiSourceDocumentGap> {
    if let Some(name) = &node.template_name {
        templates.insert(name.to_ascii_lowercase(), node.clone());
    }
    node.children
        .iter()
        .try_for_each(|child| collect_named_ui_source_templates(child, templates))
}

pub(super) fn remove_inline_ui_source_templates_lowered_as_reusable_row_documents(
    source_documents: &mut BTreeMap<String, SourceUiDocument>,
) {
    fn remove_inline_template_by_name(node: &mut SourceUiNode, template_name: &str) {
        node.children
            .retain(|child| child.template_name.as_deref() != Some(template_name));
        node.children
            .iter_mut()
            .for_each(|child| remove_inline_template_by_name(child, template_name));
    }

    source_documents.values_mut().for_each(|source_document| {
        for reusable_row_template_name in [
            "openzt2_zoopedia_toc_entry",
            "openzt2_scenario_objective_row",
        ] {
            remove_inline_template_by_name(&mut source_document.root, reusable_row_template_name);
        }
    });
}

pub(super) fn collect_reusable_ui_list_row_template_names(
    source_documents: &BTreeMap<String, SourceUiDocument>,
    all_named_templates: &BTreeMap<String, SourceUiNode>,
) -> BTreeSet<String> {
    fn collect_reusable_row_template_names_from_node(
        node: &SourceUiNode,
        all_named_templates: &BTreeMap<String, SourceUiNode>,
        reusable_row_template_names: &mut BTreeSet<String>,
    ) {
        match &node.widget {
            SourceUiWidgetData::List(list) | SourceUiWidgetData::DropList { list, .. } => {
                reusable_row_template_names.extend(list.row_template.iter().cloned());
            }
            SourceUiWidgetData::TypeList(_) => {
                if all_named_templates.contains_key("purchaseicon") {
                    reusable_row_template_names.insert("purchaseicon".to_owned());
                }
            }
            SourceUiWidgetData::ToggleSet { .. }
                if node.name.as_deref() == Some("ZTAdoptionPanel") =>
            {
                if all_named_templates.contains_key("adopt") {
                    reusable_row_template_names.insert("adopt".to_owned());
                }
            }
            _ => {}
        }
        node.children.iter().for_each(|child| {
            collect_reusable_row_template_names_from_node(
                child,
                all_named_templates,
                reusable_row_template_names,
            );
        });
    }

    let mut reusable_row_template_names = BTreeSet::new();
    source_documents.values().for_each(|source_document| {
        collect_reusable_row_template_names_from_node(
            &source_document.root,
            all_named_templates,
            &mut reusable_row_template_names,
        );
    });
    reusable_row_template_names
}

pub(super) fn add_reusable_ui_list_row_templates_to_selected_template_set(
    selected_templates: &mut BTreeMap<String, SourceUiNode>,
    all_named_templates: &BTreeMap<String, SourceUiNode>,
    reusable_row_template_names: &BTreeSet<String>,
) {
    reusable_row_template_names
        .iter()
        .for_each(|template_name| {
            if let Some(template) = all_named_templates.get(template_name) {
                selected_templates.insert(template_name.clone(), template.clone());
            }
        });
}
