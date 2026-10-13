use std::hash::{Hash, Hasher};

use bevy::{prelude::*, render::storage::ShaderBuffer};

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};
use crate::plugins::{
    camera::camera_runtime_state_types::ZooCamera,
    information::entity_selection_types::SelectedEntity,
    placement::placed_object_types::PlacedObjectDefinitionReference,
};
use openzt2_game_assets::material::runtime::{
    effect_pass_gpu_data::EffectPassMaterial,
    resolved_effect_transform_bindings::EffectRenderViewTransforms,
};

#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "evidence borrows the existing material, hierarchy, buffer and camera owners"
)]
pub(super) fn collect_fixed_transform_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    selected: Res<SelectedEntity>,
    children: Query<&Children>,
    placed: Query<&PlacedObjectDefinitionReference>,
    renderables: Query<(Entity, &MeshMaterial3d<EffectPassMaterial>, &ViewVisibility)>,
    cameras: Query<(&GlobalTransform, &Projection, Has<ZooCamera>)>,
    materials: Res<Assets<EffectPassMaterial>>,
    buffers: Res<Assets<ShaderBuffer>>,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    for name in [
        "reflection_selected_is_bathroom",
        "reflection_draws",
        "reflection_input_matches_view",
        "reflection_view_identity",
        "reflection_material_identity",
        "reflection_buffer_identity",
        "reflection_buffer_present",
        "reflection_evaluations",
        "reflection_evaluation_failures",
        "reflection_buffer_writes",
        "reflection_output_rows",
        "reflection_output_columns",
        "reflection_output_components",
        "reflection_input_identity",
        "reflection_output_identity",
    ] {
        report.current_facts.insert(name.to_owned(), 0.0);
    }
    let Some(subject) = selected.0 else {
        return;
    };
    report.current_facts.insert(
        "reflection_selected_is_bathroom".to_owned(),
        f64::from(placed.get(subject).is_ok_and(|object| {
            object.definition == openzt2_game_data::AssetId::from_key("bathroomsmall_df")
        })),
    );
    let mut candidates = std::iter::once(subject)
        .chain(children.iter_descendants_depth_first::<Children>(subject))
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    let mut count = 0;
    let mut failure_count = 0;
    for entity in candidates {
        let Ok((_, handle, visibility)) = renderables.get(entity) else {
            continue;
        };
        if !visibility.get() {
            continue;
        }
        let Some(material) = materials.get(&handle.0) else {
            continue;
        };
        let (view, evaluations, failures, writes) = material.fixed_transform_runtime_measurements();
        let Some((stage, _, input)) = material
            .fixed_transform_parameter_inputs()
            .find(|(_, name, _)| name.eq_ignore_ascii_case("ViewToWorld"))
        else {
            continue;
        };
        failure_count += failures;
        let Some(view) = view else {
            continue;
        };
        let Ok((transform, projection, is_zoo)) = cameras.get(view) else {
            continue;
        };
        if !is_zoo {
            continue;
        } // primary draw only; additional views own distinct buffers
        count += 1;
        if count != 1 {
            continue;
        }
        let current_input = EffectRenderViewTransforms::from_camera(transform, projection)
            .view_to_world_components();
        let matches = input.len() == current_input.len()
            && input
                .iter()
                .zip(current_input)
                .all(|(a, b)| a.to_bits() == b.to_bits());
        let output = material
            .fixed_transform_stage_values(stage)
            .expect("retained binding stage belongs to the fixed-transform bank")
            .to_cols_array();
        let layout = material
            .fixed_transform_output_layouts()
            .find(|(candidate, _)| *candidate == stage)
            .expect("input and layout belong to the same retained binding")
            .1;
        for (name, value) in [
            ("reflection_input_matches_view", f64::from(matches)),
            ("reflection_view_identity", fingerprint(&view)),
            ("reflection_material_identity", fingerprint(&handle.0.id())),
            (
                "reflection_buffer_identity",
                fingerprint(&material.fixed_transform_buffer_handle().id()),
            ),
            (
                "reflection_buffer_present",
                f64::from(buffers.contains(material.fixed_transform_buffer_handle().id())),
            ),
            ("reflection_evaluations", count_value(evaluations)),
            ("reflection_evaluation_failures", count_value(failures)),
            ("reflection_buffer_writes", count_value(writes)),
            ("reflection_output_rows", f64::from(layout.row_count)),
            ("reflection_output_columns", f64::from(layout.column_count)),
            (
                "reflection_output_components",
                f64::from(layout.component_count),
            ),
            (
                "reflection_input_identity",
                fingerprint(&input.iter().map(|v| v.to_bits()).collect::<Vec<_>>()),
            ),
            (
                "reflection_output_identity",
                fingerprint(&output.map(f32::to_bits)),
            ),
        ] {
            report.current_facts.insert(name.to_owned(), value);
        }
    }
    report
        .current_facts
        .insert("reflection_draws".to_owned(), f64::from(count));
    report.current_facts.insert(
        "reflection_evaluation_failures".to_owned(),
        count_value(failure_count),
    );
}

// Exact integer facts fit in f64; identity fingerprints support same-process
// comparisons and do not claim pixel correctness or replace native matrix math.
fn fingerprint(value: &impl Hash) -> f64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hash);
    f64::from((hash.finish() & u64::from(u32::MAX)) as u32)
}

fn count_value(value: u64) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}
