use crate::assets::source_document::ui::model::SourceUiNode;
use crate::assets::source_document::ui::parser::SourceUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_asset_dependency_resolution::{
    cursor_texture_dependency, texture_dependency,
};
use crate::assets::ui_document::source::lower::authored_ui_node_property_binding_resolution;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::{
    lower_node, BuildOutput,
};
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use crate::assets::ui_document::source::lower::authored_ui_template_resolution::{
    collect_templates, logical_size,
};
use openzt2_game_data::ui_document::document::{
    UiDependency, UiDependencyKind, UiDocument, UiDocumentRole,
};
use openzt2_game_data::ui_document::gameplay_pointer_presentation::{
    UiConstructionPlacementPreviewDefinition, UiGameplayInteractionCursorDefinition,
};
use openzt2_game_data::ui_document::image_selection::UiImageSelectionGroupDefinition;
use openzt2_game_data::AssetId;
use std::collections::{BTreeMap, BTreeSet};
use std::io;

// Direct compilation of Blue Fang UI vocabulary into typed UI tables.

pub(super) const SOURCE_POINTS_TO_PIXELS: f32 = 96.0 / 72.0;

pub(super) const SELECTED_ENTITY_INVENTORY_ROW_DOCUMENT: &str =
    "ui/fragment/ui/layout/iconitem.xml";
pub(super) const SELECTED_ANIMAL_NEED_ROW_DOCUMENT: &str = "ui/fragment/ui/layout/needsitem.xml";
pub(in crate::assets::ui_document::source) const DISPLAY_RESOLUTION_ROW_DOCUMENT: &str = "ui/fragment/ui/layout/resolution.xml";
pub(in crate::assets::ui_document::source) const PROFILE_ROW_DOCUMENT: &str = "ui/fragment/ui/layout/profileentry.xml";
pub(in crate::assets::ui_document::source) const CAMPAIGN_ROW_DOCUMENT: &str =
    "ui/fragment/ui/layout/campaign/campaign.xml";
pub(in crate::assets::ui_document::source) const CAMPAIGN_SCENARIO_ROW_DOCUMENT: &str =
    "ui/fragment/ui/layout/scenariobutton.xml";
pub(in crate::assets::ui_document::source) const FINANCE_CATEGORY_ROW_DOCUMENT: &str =
    "ui/fragment/ui/layout/econlistcategory.xml";
pub(in crate::assets::ui_document::source) const FINANCE_VALUE_ROW_DOCUMENT: &str =
    "ui/fragment/ui/layout/econlistitem.xml";
pub(in crate::assets::ui_document::source) const OVERVIEW_LEGEND_ROW_DOCUMENT: &str =
    "ui/fragment/ui/layout/legenditem.xml";
pub(in crate::assets::ui_document::source) const NATIVE_FINANCE_ROW_DOCUMENTS: &[(&str, &str)] = &[
    ("econlistcategory.xml", FINANCE_CATEGORY_ROW_DOCUMENT),
    ("econlistitem.xml", FINANCE_VALUE_ROW_DOCUMENT),
    (
        "balancesheetitem.xml",
        "ui/fragment/ui/layout/balancesheetitem.xml",
    ),
    (
        "balancesheetmonth.xml",
        "ui/fragment/ui/layout/balancesheetmonth.xml",
    ),
    (
        "buildinglistitem.xml",
        "ui/fragment/ui/layout/buildinglistitem.xml",
    ),
    (
        "donationboxlistitem.xml",
        "ui/fragment/ui/layout/donationboxlistitem.xml",
    ),
    (
        "donationsbyspeciesitem.xml",
        "ui/fragment/ui/layout/donationsbyspeciesitem.xml",
    ),
    (
        "showdonationentry.xml",
        "ui/fragment/ui/layout/showdonationentry.xml",
    ),
];
pub(super) const PHOTO_CAMERA_ROLL_ROW_DOCUMENT: &str = "ui/fragment/ui/layout/exposure.xml";
pub(super) const PHOTO_ALBUM_CHOICE_ROW_DOCUMENT: &str = "ui/fragment/ui/layout/albumsentry.xml";
pub(in crate::assets::ui_document::source) const MULTILIST_ROW_DOCUMENTS: [(&str, &str); 6] = [
    (
        "animals multi list",
        "ui/fragment/ui/layout/multiinfo/multilistanimal.xml",
    ),
    (
        "guest multi list",
        "ui/fragment/ui/layout/multiinfo/multilistguest.xml",
    ),
    (
        "staff multi list",
        "ui/fragment/ui/layout/multiinfo/multiliststaff.xml",
    ),
    (
        "building multi list",
        "ui/fragment/ui/layout/multiinfo/multilistbuilding.xml",
    ),
    (
        "donation box multi list",
        "ui/fragment/ui/layout/multiinfo/multilistdonationbox.xml",
    ),
    (
        "vehicle multi list",
        "ui/fragment/ui/layout/multiinfo/multilistvehicle.xml",
    ),
];

#[derive(Clone, Debug)]
pub(in crate::assets::ui_document::source) struct AuthoredUiDocument {
    pub virtual_path: String,
    pub id: AssetId,
    pub role: UiDocumentRole,
    pub source: SourceUiDocument,
    pub templates: BTreeMap<String, SourceUiNode>,
    pub image_selections: Vec<AuthoredUiImageSelection>,
    pub interaction_cursors: Option<AuthoredUiInteractionCursors>,
    pub placement_preview: Option<AuthoredUiPlacementPreview>,
    /// Names of nodes in other documents, indexed by their document role.
    pub role_targets: BTreeMap<String, UiDocumentRole>,
    /// Winning archive paths for referenced assets.
    pub resolved_dependencies: UiResolvedDependencies,
}

#[derive(Clone, Debug)]
pub(in crate::assets::ui_document::source) struct AuthoredUiInteractionCursors {
    pub overhead: String,
    pub placement_default: String,
    pub placement_pickup: String,
    pub placement_rotate: String,
    pub fence_default: String,
    pub fence_gate: String,
    pub path: String,
    pub elevated_path: String,
    pub biome_default: String,
    pub biome_paint: String,
    pub biome_invalid: String,
    pub terrain: String,
    pub tank: String,
    pub selection_default: String,
    pub selection_pickup: String,
    pub selection_rotate: String,
    pub delete: String,
}

#[derive(Clone, Debug)]
pub(in crate::assets::ui_document::source) struct AuthoredUiPlacementPreview {
    pub footprint_valid: String,
    pub footprint_valid_small: String,
    pub footprint_invalid: String,
    pub footprint_invalid_small: String,
    pub grid_large_cardinal: String,
    pub grid_large_diagonal: String,
    pub grid_large_radius: f32,
    pub grid_medium_cardinal: String,
    pub grid_medium_diagonal: String,
    pub grid_medium_radius: f32,
    pub grid_small_cardinal: String,
    pub grid_small_diagonal: String,
    pub grid_small_radius: f32,
    pub model_valid_srgba: [u8; 4],
    pub model_invalid_srgba: [u8; 4],
    pub fence_valid_srgba: [u8; 4],
    pub fence_invalid_srgba: [u8; 4],
    pub footprint_valid_srgba: [u8; 4],
    pub footprint_invalid_srgba: [u8; 4],
    pub grid_srgba: [u8; 4],
}

#[derive(Clone, Debug, Default)]
pub(in crate::assets::ui_document::source) struct UiResolvedDependencies {
    pub images: BTreeMap<String, String>,
    /// Texture dimensions used to resolve `LAST_W` and `LAST_H` atlas tokens.
    pub image_dimensions: BTreeMap<String, [u32; 2]>,
    /// Missing static images render empty. Image-changing commands still require a match.
    pub absent_visual_images: BTreeSet<String>,
    pub models: BTreeMap<String, String>,
    pub audio: BTreeMap<String, String>,
    pub documents: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub(in crate::assets::ui_document::source) struct AuthoredUiImageSelection {
    pub targets: Vec<String>,
    pub candidates: Vec<AssetId>,
}

/// Stable identity used for an authored reusable list-row document.
pub(in crate::assets::ui_document::source) fn list_row_document_path(template: &str) -> String {
    let key = template
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase();
    format!("ui/template/{key}#row")
}

pub(in crate::assets::ui_document::source) const ADOPTION_SLOT_ROW_DOCUMENT: &str =
    "ui/native/adoption-slot#row";

pub(in crate::assets::ui_document::source) fn lower_ui_document(
    input: &AuthoredUiDocument,
) -> io::Result<UiDocument> {
    if let Some(diagnostic) = input.source.diagnostics.first() {
        return Err(invalid_at(
            input,
            format!("unresolved source UI diagnostic: {diagnostic:?}"),
        ));
    }
    if !input.source.skins.is_empty() {
        return Err(invalid_at(
            input,
            "UI skins must be resolved before semantic lowering",
        ));
    }
    let logical_size = logical_size(&input.source.root, input.role)?;
    let mut templates = input.templates.clone();
    for (name, template) in collect_templates(&input.source.root)? {
        templates.insert(name, template);
    }
    let mut output = BuildOutput::default();
    output.simulation_paused_visible_nodes = authored_ui_node_property_binding_resolution::collect_authored_simulation_paused_visibility_target_names(&input.source.root);
    let mut stack = Vec::new();
    lower_node(
        &input.source.root,
        u32::MAX,
        None,
        input,
        &templates,
        &mut stack,
        &mut output,
    )?;
    output.dependencies.remove(&[0; 16]);

    let gameplay_interaction_cursors = input
        .interaction_cursors
        .as_ref()
        .map(|policy| lower_interaction_cursors(policy, &mut output, input))
        .transpose()?
        .unwrap_or_default();
    let construction_placement_preview = input
        .placement_preview
        .as_ref()
        .map(|policy| lower_placement_preview(policy, &mut output, input))
        .transpose()?
        .unwrap_or_default();

    let mut image_selection_groups = Vec::with_capacity(input.image_selections.len());
    for selection in &input.image_selections {
        let target_node_ids = selection
            .targets
            .iter()
            .map(|name| UiDocumentRole::node_id(input.role, name))
            .collect();
        let candidate_image_ids = selection.candidates.clone();
        selection.candidates.iter().for_each(|candidate| {
            output.dependencies.insert(candidate.0);
        });
        image_selection_groups.push(UiImageSelectionGroupDefinition {
            target_node_ids,
            candidate_image_ids,
        });
    }

    Ok(UiDocument {
        id: input.id,
        role: input.role,
        logical_size,
        nodes: output.nodes,
        gameplay_interaction_cursors,
        construction_placement_preview,
        image_selection_groups,
        dependencies: ui_dependencies(
            input,
            &output.dependencies,
            &output.interactive_textures,
            &output.explicit_scenes,
        ),
    })
}

fn ui_dependencies(
    input: &AuthoredUiDocument,
    used: &BTreeSet<[u8; 16]>,
    interactive_textures: &BTreeSet<[u8; 16]>,
    explicit_scenes: &BTreeMap<[u8; 16], String>,
) -> Vec<UiDependency> {
    fn insert_dependency(
        dependencies: &mut BTreeMap<[u8; 16], UiDependency>,
        used: &BTreeSet<[u8; 16]>,
        path: &str,
        kind: UiDependencyKind,
    ) {
        let path = path.to_owned();
        let id = AssetId::from_virtual_path(&path);
        if used.contains(&id.0) {
            dependencies.insert(id.0, UiDependency { id, path, kind });
        }
    }

    let mut dependencies = BTreeMap::<[u8; 16], UiDependency>::new();
    input
        .resolved_dependencies
        .images
        .values()
        .for_each(|path| {
            let id = AssetId::from_virtual_path(path);
            insert_dependency(
                &mut dependencies,
                used,
                path,
                if interactive_textures.contains(&id.0) {
                    UiDependencyKind::InteractiveTexture
                } else {
                    UiDependencyKind::Texture
                },
            );
        });
    input
        .resolved_dependencies
        .models
        .values()
        .for_each(|path| insert_dependency(&mut dependencies, used, path, UiDependencyKind::Scene));
    input
        .resolved_dependencies
        .documents
        .values()
        .for_each(|path| {
            let direct = AssetId::from_virtual_path(path);
            if used.contains(&direct.0) {
                dependencies.insert(
                    direct.0,
                    UiDependency {
                        id: direct,
                        path: path.clone(),
                        kind: UiDependencyKind::Document,
                    },
                );
            }
            // Fragment identities use the normalized key, but archive winners
            // keep their authored case, such as `UI/layout/campaign/campaign.xml`.
            let fragment = AssetId::from_virtual_path(&format!(
                "ui/fragment/{}",
                crate::assets::source_document::path::AssetPath::new(path).key()
            ));
            if used.contains(&fragment.0) {
                dependencies.insert(
                    fragment.0,
                    UiDependency {
                        id: fragment,
                        path: path.clone(),
                        kind: UiDependencyKind::Document,
                    },
                );
            }
        });
    input.templates.keys().for_each(|template_name| {
        insert_dependency(
            &mut dependencies,
            used,
            &list_row_document_path(template_name),
            UiDependencyKind::Document,
        );
    });
    insert_dependency(
        &mut dependencies,
        used,
        ADOPTION_SLOT_ROW_DOCUMENT,
        UiDependencyKind::Document,
    );
    input
        .resolved_dependencies
        .audio
        .values()
        .for_each(|path| insert_dependency(&mut dependencies, used, path, UiDependencyKind::Audio));
    explicit_scenes
        .values()
        .for_each(|path| insert_dependency(&mut dependencies, used, path, UiDependencyKind::Scene));
    dependencies.into_values().collect()
}

fn lower_placement_preview(
    policy: &AuthoredUiPlacementPreview,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<UiConstructionPlacementPreviewDefinition> {
    let texture = |value: &str, output: &mut BuildOutput| {
        texture_dependency(Some(value.to_owned()), output, input)
    };
    Ok(UiConstructionPlacementPreviewDefinition {
        footprint_valid: texture(&policy.footprint_valid, output)?,
        footprint_valid_small: texture(&policy.footprint_valid_small, output)?,
        footprint_invalid: texture(&policy.footprint_invalid, output)?,
        footprint_invalid_small: texture(&policy.footprint_invalid_small, output)?,
        grid_large_cardinal: texture(&policy.grid_large_cardinal, output)?,
        grid_large_diagonal: texture(&policy.grid_large_diagonal, output)?,
        grid_large_radius: policy.grid_large_radius,
        grid_medium_cardinal: texture(&policy.grid_medium_cardinal, output)?,
        grid_medium_diagonal: texture(&policy.grid_medium_diagonal, output)?,
        grid_medium_radius: policy.grid_medium_radius,
        grid_small_cardinal: texture(&policy.grid_small_cardinal, output)?,
        grid_small_diagonal: texture(&policy.grid_small_diagonal, output)?,
        grid_small_radius: policy.grid_small_radius,
        model_valid_srgba: policy.model_valid_srgba,
        model_invalid_srgba: policy.model_invalid_srgba,
        fence_valid_srgba: policy.fence_valid_srgba,
        fence_invalid_srgba: policy.fence_invalid_srgba,
        footprint_valid_srgba: policy.footprint_valid_srgba,
        footprint_invalid_srgba: policy.footprint_invalid_srgba,
        grid_srgba: policy.grid_srgba,
    })
}

fn lower_interaction_cursors(
    policy: &AuthoredUiInteractionCursors,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<UiGameplayInteractionCursorDefinition> {
    let cursor = |value: &str, output: &mut BuildOutput| {
        cursor_texture_dependency(Some(value.to_owned()), output, input)
    };
    Ok(UiGameplayInteractionCursorDefinition {
        overhead: cursor(&policy.overhead, output)?,
        placement_default: cursor(&policy.placement_default, output)?,
        placement_pickup: cursor(&policy.placement_pickup, output)?,
        placement_rotate: cursor(&policy.placement_rotate, output)?,
        fence_default: cursor(&policy.fence_default, output)?,
        fence_gate: cursor(&policy.fence_gate, output)?,
        path: cursor(&policy.path, output)?,
        elevated_path: cursor(&policy.elevated_path, output)?,
        biome_default: cursor(&policy.biome_default, output)?,
        biome_paint: cursor(&policy.biome_paint, output)?,
        biome_invalid: cursor(&policy.biome_invalid, output)?,
        terrain: cursor(&policy.terrain, output)?,
        tank: cursor(&policy.tank, output)?,
        selection_default: cursor(&policy.selection_default, output)?,
        selection_pickup: cursor(&policy.selection_pickup, output)?,
        selection_rotate: cursor(&policy.selection_rotate, output)?,
        delete: cursor(&policy.delete, output)?,
    })
}
