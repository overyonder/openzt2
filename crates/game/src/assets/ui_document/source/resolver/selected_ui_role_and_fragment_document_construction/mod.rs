//! Construction of role-owned and fragment-owned canonical lowerer inputs.

use std::collections::{BTreeMap, BTreeSet};

use openzt2_game_data::{ui_document::document::UiDocumentRole, AssetId};

use crate::assets::{
    source_document::{
        path::AssetPath,
        ui::{
            model::{SourceUiMetric, SourceUiNode},
            parser::SourceUiDocument,
        },
    },
    ui_document::source::lower::authored_ui_document_lowering::{
        AuthoredUiDocument, AuthoredUiImageSelection, AuthoredUiInteractionCursors,
        AuthoredUiPlacementPreview, UiResolvedDependencies,
    },
};

use super::{
    super::ui_source_document_gap::{
        UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
    },
    authored_ui_source_skin_image_resolution::retain_authored_ui_image_selections_targeting_source_document,
    resolution_profile::SelectedUiSourceResolutionProfile,
    resolved_ui_document_types::ResolvedUiRoleAndFragmentDocuments,
    ui_document_role_specialization_and_composition::{
        attach_authored_global_hotkey_mode_to_document_role,
        expose_authored_ui_surface_for_document_role,
        specialize_authored_ui_source_document_for_role,
    },
    ui_document_role_target_name_resolution::collect_unique_authored_node_name_targets_by_ui_document_role,
    ui_source_template_collection_and_selection::{
        add_reusable_ui_list_row_templates_to_selected_template_set,
        select_transitively_referenced_ui_source_templates,
    },
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::ADOPTION_SLOT_ROW_DOCUMENT;

pub(super) fn construct_selected_ui_role_and_fragment_document_lowering_inputs(
    source_documents: BTreeMap<String, SourceUiDocument>,
    all_named_templates: &BTreeMap<String, SourceUiNode>,
    reusable_row_template_names: &BTreeSet<String>,
    authored_image_selections: &[AuthoredUiImageSelection],
    interaction_cursors: Option<AuthoredUiInteractionCursors>,
    placement_preview: Option<AuthoredUiPlacementPreview>,
    resolved_dependencies: UiResolvedDependencies,
    resolution_profile: &SelectedUiSourceResolutionProfile,
) -> Result<ResolvedUiRoleAndFragmentDocuments, UiSourceDocumentGap> {
    let mut claimed_source_document_roles = BTreeMap::new();
    let global_hotkey_source = resolution_profile
        .hotkey_path
        .as_deref()
        .map(normalize_authored_ui_source_path)
        .map(|path| match source_documents.get(&path) {
            Some(source_document) => Ok((path, source_document)),
            None => Err(UiSourceDocumentGap {
                family: UiSourceDocumentFamily::Ui,
                kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                virtual_path: path,
                span: Default::default(),
                message: "resolved core UI documents has no global hotkey document".into(),
            }),
        })
        .transpose()?;
    if let Some((path, _)) = &global_hotkey_source {
        claimed_source_document_roles.insert(
            path.clone(),
            resolution_profile
                .role_hotkey_modes
                .first()
                .map_or(UiDocumentRole::Fragment, |(role, _)| *role),
        );
    }

    let role_targets = collect_unique_authored_node_name_targets_by_ui_document_role(
        &source_documents,
        resolution_profile,
    )?;
    let mut role_documents = Vec::new();
    for (role, authored_path) in &resolution_profile.role_paths {
        let path = normalize_authored_ui_source_path(authored_path);
        let source_document = source_documents
            .get(&path)
            .ok_or_else(|| UiSourceDocumentGap {
                family: UiSourceDocumentFamily::Ui,
                kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                virtual_path: path.clone(),
                span: Default::default(),
                message: format!(
                    "resolved core UI documents has no source document for role {role:?}"
                ),
            })?;
        claimed_source_document_roles.insert(path, *role);
        let mut role_source_document =
            specialize_authored_ui_source_document_for_role(source_document, *role)?;
        for (_, composition_path) in resolution_profile
            .role_compositions
            .iter()
            .filter(|(candidate_role, _)| candidate_role == role)
        {
            let composition_path = normalize_authored_ui_source_path(composition_path);
            let Some(composition_source_document) = source_documents.get(&composition_path) else {
                continue;
            };
            claimed_source_document_roles.insert(composition_path, *role);
            role_source_document
                .root
                .children
                .push(composition_source_document.root.clone());
        }
        if let Some((_, hotkey_mode)) = resolution_profile
            .role_hotkey_modes
            .iter()
            .find(|(candidate_role, _)| candidate_role == role)
        {
            let (_, global_hotkey_source_document) =
                global_hotkey_source
                    .as_ref()
                    .ok_or_else(|| UiSourceDocumentGap {
                        family: UiSourceDocumentFamily::Ui,
                        kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                        virtual_path: role_source_document.path.key(),
                        span: role_source_document.root.span,
                        message: format!(
                            "role {role:?} requires global hotkey mode {hotkey_mode:?}"
                        ),
                    })?;
            attach_authored_global_hotkey_mode_to_document_role(
                &mut role_source_document,
                *role,
                global_hotkey_source_document,
                hotkey_mode,
            )?;
        }
        if let Some((_, authored_surface_name)) = resolution_profile
            .role_surfaces
            .iter()
            .find(|(candidate_role, _)| candidate_role == role)
        {
            expose_authored_ui_surface_for_document_role(
                &mut role_source_document,
                *role,
                authored_surface_name,
            )?;
        }
        let mut selected_templates = select_transitively_referenced_ui_source_templates(
            &role_source_document.root,
            all_named_templates,
        )?;
        add_reusable_ui_list_row_templates_to_selected_template_set(
            &mut selected_templates,
            all_named_templates,
            reusable_row_template_names,
        );
        let selected_image_selections =
            retain_authored_ui_image_selections_targeting_source_document(
                &role_source_document,
                authored_image_selections,
            );
        role_documents.push(AuthoredUiDocument {
            virtual_path: role_source_document.path.key(),
            id: AssetId::from_key(&format!("ui/role/{}", role.stable_key())),
            role: *role,
            source: role_source_document,
            templates: selected_templates,
            image_selections: selected_image_selections,
            interaction_cursors: (*role == UiDocumentRole::InGameHud)
                .then(|| interaction_cursors.clone())
                .flatten(),
            placement_preview: (*role == UiDocumentRole::InGameHud)
                .then(|| placement_preview.clone())
                .flatten(),
            role_targets: role_targets.clone(),
            resolved_dependencies: resolved_dependencies.clone(),
        });
    }

    let adoption_slot_fragment = construct_native_adoption_slot_fragment_document(
        &source_documents,
        all_named_templates,
        reusable_row_template_names,
        &resolved_dependencies,
    )?;
    let mut fragment_documents = source_documents
        .into_iter()
        .filter(|(source_path, _)| {
            !claimed_source_document_roles.contains_key(source_path)
                && !source_path.starts_with("ui/template/")
        })
        .map(|(source_path, source_document)| {
            construct_path_addressed_fragment_document(
                source_path,
                source_document,
                all_named_templates,
                reusable_row_template_names,
                authored_image_selections,
                &resolved_dependencies,
            )
        })
        .collect::<Result<Vec<_>, UiSourceDocumentGap>>()?;
    fragment_documents.extend(
        reusable_row_template_names
            .iter()
            .map(|template_name| {
                construct_reusable_row_fragment_document(
                    template_name,
                    all_named_templates,
                    reusable_row_template_names,
                    &resolved_dependencies,
                )
            })
            .collect::<Result<Vec<_>, UiSourceDocumentGap>>()?,
    );
    if let Some(adoption_slot_fragment) = adoption_slot_fragment {
        fragment_documents.push(adoption_slot_fragment);
    }
    Ok(ResolvedUiRoleAndFragmentDocuments {
        roles: role_documents,
        fragments: fragment_documents,
    })
}

fn construct_native_adoption_slot_fragment_document(
    source_documents: &BTreeMap<String, SourceUiDocument>,
    all_named_templates: &BTreeMap<String, SourceUiNode>,
    reusable_row_template_names: &BTreeSet<String>,
    resolved_dependencies: &UiResolvedDependencies,
) -> Result<Option<AuthoredUiDocument>, UiSourceDocumentGap> {
    let (Some(adoption_button), Some(blank_slot), Some(adopt_icon), Some(purchase_icon_region)) = (
        source_documents.get("ui/layout/adoptbutton.xml"),
        source_documents.get("ui/layout/blankslot.xml"),
        all_named_templates.get("adopt"),
        all_named_templates
            .get("purchaseicon")
            .and_then(|template| template.region.as_ref()),
    ) else {
        return Ok(None);
    };
    let mut source_document = adoption_button.clone();
    source_document.path = AssetPath::new(ADOPTION_SLOT_ROW_DOCUMENT);
    for (name, y) in [
        ("Primary Adoption Offer", 0.0),
        ("Secondary Adoption Offer", 48.0),
    ] {
        let mut icon = adopt_icon.clone();
        icon.name = Some(name.to_owned());
        icon.template_name = None;
        let mut region = purchase_icon_region.clone();
        region.x = SourceUiMetric::Number(0.0);
        region.y = SourceUiMetric::Number(y);
        icon.region = Some(region);
        source_document.root.children.push(icon);
    }
    let mut locked_slot = blank_slot.root.clone();
    locked_slot.name = Some("Locked Adoption Slot".to_owned());
    source_document.root.children.push(locked_slot);
    let mut selected_templates = select_transitively_referenced_ui_source_templates(
        &source_document.root,
        all_named_templates,
    )?;
    add_reusable_ui_list_row_templates_to_selected_template_set(
        &mut selected_templates,
        all_named_templates,
        reusable_row_template_names,
    );
    Ok(Some(AuthoredUiDocument {
        virtual_path: ADOPTION_SLOT_ROW_DOCUMENT.to_owned(),
        id: AssetId::from_virtual_path(ADOPTION_SLOT_ROW_DOCUMENT),
        role: UiDocumentRole::Fragment,
        source: source_document,
        templates: selected_templates,
        image_selections: Vec::new(),
        interaction_cursors: None,
        placement_preview: None,
        role_targets: BTreeMap::new(),
        resolved_dependencies: resolved_dependencies.clone(),
    }))
}

fn construct_reusable_row_fragment_document(
    template_name: &str,
    all_named_templates: &BTreeMap<String, SourceUiNode>,
    reusable_row_template_names: &BTreeSet<String>,
    resolved_dependencies: &UiResolvedDependencies,
) -> Result<AuthoredUiDocument, UiSourceDocumentGap> {
    let mut root = all_named_templates
        .get(template_name)
        .cloned()
        .ok_or_else(|| UiSourceDocumentGap {
            family: UiSourceDocumentFamily::Ui,
            kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
            virtual_path: "ui/template-resolution".into(),
            span: Default::default(),
            message: format!("list row template {template_name:?} does not resolve"),
        })?;
    root.template_name = None;
    let virtual_path =
        super::super::lower::authored_ui_document_lowering::list_row_document_path(template_name);
    let source_document = SourceUiDocument {
        path: AssetPath::new(&virtual_path),
        root,
        skins: Vec::new(),
        cursor_directory: None,
        named_event_lists: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut selected_templates = select_transitively_referenced_ui_source_templates(
        &source_document.root,
        all_named_templates,
    )?;
    add_reusable_ui_list_row_templates_to_selected_template_set(
        &mut selected_templates,
        all_named_templates,
        reusable_row_template_names,
    );
    Ok(AuthoredUiDocument {
        virtual_path: virtual_path.clone(),
        id: AssetId::from_virtual_path(&virtual_path),
        role: UiDocumentRole::Fragment,
        source: source_document,
        templates: selected_templates,
        image_selections: Vec::new(),
        interaction_cursors: None,
        placement_preview: None,
        role_targets: BTreeMap::new(),
        resolved_dependencies: resolved_dependencies.clone(),
    })
}

fn construct_path_addressed_fragment_document(
    source_path: String,
    source_document: SourceUiDocument,
    all_named_templates: &BTreeMap<String, SourceUiNode>,
    reusable_row_template_names: &BTreeSet<String>,
    authored_image_selections: &[AuthoredUiImageSelection],
    resolved_dependencies: &UiResolvedDependencies,
) -> Result<AuthoredUiDocument, UiSourceDocumentGap> {
    let mut selected_templates = select_transitively_referenced_ui_source_templates(
        &source_document.root,
        all_named_templates,
    )?;
    add_reusable_ui_list_row_templates_to_selected_template_set(
        &mut selected_templates,
        all_named_templates,
        reusable_row_template_names,
    );
    let selected_image_selections = retain_authored_ui_image_selections_targeting_source_document(
        &source_document,
        authored_image_selections,
    );
    Ok(AuthoredUiDocument {
        virtual_path: source_path.clone(),
        id: AssetId::from_virtual_path(&format!("ui/fragment/{source_path}")),
        role: UiDocumentRole::Fragment,
        source: source_document,
        templates: selected_templates,
        image_selections: selected_image_selections,
        interaction_cursors: None,
        placement_preview: None,
        role_targets: BTreeMap::new(),
        resolved_dependencies: resolved_dependencies.clone(),
    })
}

fn normalize_authored_ui_source_path(authored_path: &str) -> String {
    authored_path.replace('\\', "/").to_ascii_lowercase()
}
