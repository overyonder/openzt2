//! Authored UI source-skin image selection and replacement resolution.

use std::collections::{BTreeMap, BTreeSet};

use openzt2_game_data::AssetId;

use crate::assets::{
    source_document::{
        ordered_source_document_types::OrderedSourceDocumentSpan,
        path::AssetPath,
        ui::{model::SourceUiNode, parser::SourceUiDocument},
    },
    ui_document::source::lower::authored_ui_document_lowering::AuthoredUiImageSelection,
};

use super::{
    super::ui_source_document_gap::{
        UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
    },
    selected_ui_dependency_winner_resolution::resolve_selected_ui_dependency_winner_path,
};

pub(super) fn retain_authored_ui_image_selections_targeting_source_document(
    source: &SourceUiDocument,
    selections: &[AuthoredUiImageSelection],
) -> Vec<AuthoredUiImageSelection> {
    selections
        .iter()
        .filter_map(|selection| {
            let targets = selection
                .targets
                .iter()
                .filter(|target| source_node_tree_contains_authored_name(&source.root, target))
                .cloned()
                .collect::<Vec<_>>();
            (!targets.is_empty()).then(|| AuthoredUiImageSelection {
                targets,
                candidates: selection.candidates.clone(),
            })
        })
        .collect()
}

fn source_node_tree_contains_authored_name(node: &SourceUiNode, target: &str) -> bool {
    node.name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case(target))
        || node
            .children
            .iter()
            .any(|child| source_node_tree_contains_authored_name(child, target))
}

pub(super) fn resolve_authored_file_and_directory_skin_image_selections(
    sources: &BTreeMap<String, SourceUiDocument>,
    available_assets: &[String],
) -> Result<Vec<AuthoredUiImageSelection>, UiSourceDocumentGap> {
    let texture_paths = available_assets
        .iter()
        .map(|path| path.replace('\\', "/").to_ascii_lowercase())
        .filter(|path| {
            matches!(
                path.rsplit_once('.').map(|(_, extension)| extension),
                Some("dds" | "tga" | "bmp" | "jpg" | "jpeg" | "png")
            )
        })
        .collect::<Vec<_>>();
    let asset_id = |path: &str| AssetId::from_virtual_path(path);
    let mut selections = Vec::new();
    for skin in sources.values().flat_map(|source| source.skins.iter()) {
        let replacement_type = skin.replacement_type.as_deref();
        if !matches!(replacement_type, Some("file" | "directory")) {
            continue;
        }
        let directory = skin.directory.as_ref().ok_or_else(|| UiSourceDocumentGap {
            family: UiSourceDocumentFamily::Ui,
            kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
            virtual_path: "ui/skin-resolution".into(),
            span: OrderedSourceDocumentSpan::default(),
            message: format!(
                "{replacement_type:?} skin {:?} has no source directory",
                skin.name
            ),
        })?;
        let prefix = directory.key().trim_end_matches('/').to_owned() + "/";
        if replacement_type == Some("file") {
            let mut seen = BTreeSet::new();
            let candidates = texture_paths
                .iter()
                .filter(|path| path.starts_with(&prefix))
                .map(|path| asset_id(path))
                .filter(|candidate| seen.insert(candidate.0))
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Err(UiSourceDocumentGap {
                    family: UiSourceDocumentFamily::Ui,
                    kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                    virtual_path: "ui/skin-resolution".into(),
                    span: OrderedSourceDocumentSpan::default(),
                    message: format!(
                        "file skin {:?} resolved no texture candidates under {prefix}",
                        skin.name
                    ),
                });
            }
            selections.push(AuthoredUiImageSelection {
                targets: skin
                    .replacements
                    .iter()
                    .map(|replacement| replacement.target_name.clone())
                    .collect(),
                candidates,
            });
            continue;
        }

        // A directory skin is a set of parallel alternatives. Each authored
        // image name is resolved to the same ordered child-directory cohort;
        // the runtime's document seed therefore selects one coherent skin
        // without retaining source paths or a skin manager.
        let mut replacements = BTreeMap::<String, Vec<String>>::new();
        for replacement in &skin.replacements {
            let Some(image) = replacement.image.as_ref() else {
                continue;
            };
            replacements
                .entry(image.key())
                .or_default()
                .push(replacement.target_name.clone());
        }
        let candidate_by_image = replacements
            .keys()
            .map(|image| {
                let suffix = format!("/{image}");
                let candidates = texture_paths
                    .iter()
                    .filter_map(|path| {
                        path.strip_prefix(&prefix)
                            .and_then(|relative| relative.strip_suffix(&suffix))
                            .filter(|cohort| !cohort.is_empty())
                            .map(|cohort| (cohort.to_owned(), path.as_str()))
                    })
                    .collect::<BTreeMap<_, _>>();
                (image.clone(), candidates)
            })
            .collect::<BTreeMap<_, _>>();
        let mut seen_cohorts = BTreeSet::new();
        let complete_cohorts = texture_paths
            .iter()
            .filter_map(|path| {
                path.strip_prefix(&prefix).and_then(|relative| {
                    relative
                        .split_once('/')
                        .map(|(cohort, _)| cohort.to_owned())
                })
            })
            .filter(|cohort| seen_cohorts.insert(cohort.clone()))
            .filter(|cohort| {
                candidate_by_image
                    .values()
                    .all(|candidates| candidates.contains_key(cohort))
            })
            .collect::<Vec<_>>();
        if !replacements.is_empty() && complete_cohorts.is_empty() {
            return Err(UiSourceDocumentGap {
                family: UiSourceDocumentFamily::Ui,
                kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                virtual_path: "ui/skin-resolution".into(),
                span: OrderedSourceDocumentSpan::default(),
                message: format!(
                    "directory skin {:?} has no complete image cohort under {prefix}",
                    skin.name
                ),
            });
        }
        for (image, targets) in replacements {
            let candidates_for_image = &candidate_by_image[&image];
            let candidates = complete_cohorts
                .iter()
                .map(|cohort| asset_id(candidates_for_image[cohort]))
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Err(UiSourceDocumentGap {
                    family: UiSourceDocumentFamily::Ui,
                    kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                    virtual_path: "ui/skin-resolution".into(),
                    span: OrderedSourceDocumentSpan::default(),
                    message: format!(
                        "directory skin {:?} resolved no candidates for {image:?} under {prefix}",
                        skin.name
                    ),
                });
            }
            selections.push(AuthoredUiImageSelection {
                targets,
                candidates,
            });
        }
    }
    Ok(selections)
}

pub(super) fn apply_authored_selected_skin_replacements_to_source_documents(
    sources: &mut BTreeMap<String, SourceUiDocument>,
    source_order: &[String],
    theme: Option<&str>,
    resolved_images: &BTreeMap<String, String>,
) -> Result<(), UiSourceDocumentGap> {
    let replacements = source_order
        .iter()
        .filter_map(|owner| sources.get(owner).map(|source| (owner, source)))
        .flat_map(|(owner, source)| source.skins.iter().map(move |skin| (owner, skin)))
        .filter(|(_, skin)| !matches!(skin.replacement_type.as_deref(), Some("file" | "directory")))
        .flat_map(|(owner, skin)| {
            let themed = theme.and_then(|wanted| {
                skin.themes
                    .iter()
                    .find(|candidate| candidate.name.eq_ignore_ascii_case(wanted))
            });
            let replacements = themed
                .map(|theme| theme.replacements.as_slice())
                .unwrap_or(skin.replacements.as_slice());
            replacements
                .iter()
                .cloned()
                .map(move |replacement| (owner.clone(), replacement))
        })
        .collect::<Vec<_>>();
    for (owner, mut replacement) in replacements
        .into_iter()
        .filter(|(_, replacement)| replacement.image.is_some())
    {
        let image = replacement.image.as_ref().expect("filtered image");
        let winner =
            resolve_selected_ui_dependency_winner_path(&owner, image.as_str(), resolved_images)
                .ok_or_else(|| UiSourceDocumentGap {
                    family: UiSourceDocumentFamily::Ui,
                    kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                    virtual_path: owner.to_owned(),
                    span: OrderedSourceDocumentSpan::default(),
                    message: format!(
                        "skin replacement image {:?} has no planner-selected texture winner",
                        image.as_str()
                    ),
                })?;
        replacement.image = Some(AssetPath::new(winner));
        let changed = sources
            .values_mut()
            .map(|source| {
                apply_authored_skin_replacement_to_source_node(
                    &mut source.root,
                    &replacement.target_name,
                    replacement.image.as_ref().expect("fixed replacement"),
                )
            })
            .sum::<usize>();
        if changed == 0 {
            return Err(UiSourceDocumentGap {
                family: UiSourceDocumentFamily::Ui,
                kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                virtual_path: "ui/skin-resolution".into(),
                span: OrderedSourceDocumentSpan::default(),
                message: format!(
                    "skin replacement target {:?} does not name a node or visual image in its resolved document",
                    replacement.target_name
                ),
            });
        }
    }
    sources.values_mut().for_each(|source| source.skins.clear());
    Ok(())
}

fn apply_authored_skin_replacement_to_source_node(
    node: &mut SourceUiNode,
    target: &str,
    image: &AssetPath,
) -> usize {
    let target_key = target.replace('\\', "/").to_ascii_lowercase();
    let mut changed = 0;
    if node
        .name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case(target))
    {
        if let Some(default) = node
            .aspect
            .as_mut()
            .and_then(|aspect| aspect.default.as_mut())
        {
            default.image = Some(image.clone());
            changed += 1;
        }
    }
    if let Some(aspect) = &mut node.aspect {
        for visual in aspect
            .default
            .iter_mut()
            .chain(aspect.standard.iter_mut().map(|entry| &mut entry.visual))
            .chain(aspect.alternate.iter_mut().map(|entry| &mut entry.visual))
        {
            if visual.image.as_ref().is_some_and(|path| {
                let key = path.key();
                key == target_key || key.rsplit('/').next() == Some(target_key.as_str())
            }) {
                visual.image = Some(image.clone());
                changed += 1;
            }
        }
    }
    changed
        + node
            .children
            .iter_mut()
            .map(|child| apply_authored_skin_replacement_to_source_node(child, target, image))
            .sum::<usize>()
}
