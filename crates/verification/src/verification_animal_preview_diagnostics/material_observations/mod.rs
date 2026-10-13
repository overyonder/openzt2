use bevy::{material::AlphaMode, mesh::VertexAttributeValues, pbr::Material, prelude::*};

use super::{AnimalPreviewCaptureQueries, PreviewDrawObservationItem, ProjectedPrefabTintMesh};

impl AnimalPreviewCaptureQueries<'_, '_> {
    pub(super) fn log_tint_projection(
        &self,
        entity: Entity,
        draw: &PreviewDrawObservationItem<'_, '_>,
        mesh: Option<&Mesh>,
    ) {
        let tint = std::iter::once(entity)
            .chain(self.parents.iter_ancestors(entity))
            .find_map(|ancestor| {
                self.tints
                    .get(ancestor)
                    .ok()
                    .map(|tint| (ancestor, tint.0.to_srgba()))
            });
        let projection = draw
            .tint_projection
            .map(ProjectedPrefabTintMesh::mesh_and_authored_colors);
        let attribute = mesh.and_then(|mesh| mesh.attribute(Mesh::ATTRIBUTE_COLOR));
        let colors = match attribute {
            Some(VertexAttributeValues::Float32x4(colors)) => Some(colors.as_slice()),
            _ => None,
        };
        info!(target: "openzt2_verification", tint_owner = ?tint.map(|(owner, _)| owner),
            effective_tint_srgba = ?tint.map(|(_, tint)| [tint.red, tint.green, tint.blue, tint.alpha]),
            tint_finite = ?tint.map(|(_, tint)| [tint.red, tint.green, tint.blue, tint.alpha].into_iter().all(f32::is_finite)),
            drawn_mesh = ?draw.mesh.0.id(), projected_mesh = ?projection.map(|(mesh, _)| mesh),
            draws_projected_mesh = ?projection.map(|(mesh, _)| mesh == draw.mesh.0.id()),
            application_time_tint = "not retained; effective tint is current ancestor state",
            source_mesh_identity = "not retained by tint owner; no source association inferred from vertex count",
            mesh_loaded = mesh.is_some(), color_attribute_present = attribute.is_some(),
            color_attribute_float32x4 = colors.is_some(),
            "animal preview effective tint and retained mesh projection association");
        log_vertex_color_summary(
            "retained authored tint input",
            projection.map(|(_, colors)| colors),
        );
        log_vertex_color_summary("current draw mesh", colors);
        if let Some((projected, _)) =
            projection.filter(|(projected, _)| *projected != draw.mesh.0.id())
        {
            let projected_colors = self
                .meshes
                .as_ref()
                .and_then(|meshes| meshes.get(projected))
                .and_then(|mesh| match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
                    Some(VertexAttributeValues::Float32x4(colors)) => Some(colors.as_slice()),
                    _ => None,
                });
            log_vertex_color_summary(
                "retained projected mesh differs from draw",
                projected_colors,
            );
        }
    }

    pub(super) fn log_materials(&self, draw: &PreviewDrawObservationItem<'_, '_>) {
        info!(target: "openzt2_verification", standard = ?draw.standard.map(|material| material.0.id()),
            effect = ?draw.effect.map(|material| material.0.id()), "animal preview CPU material handles");
        if let Some(handle) = draw.standard {
            let material = self
                .standard_materials
                .as_ref()
                .and_then(|assets| assets.get(&handle.0));
            info!(target: "openzt2_verification", material = ?handle.0.id(), loaded = material.is_some(),
                material_path = ?self.server.get_path(handle.0.id()),
                load_state = ?self.server.get_load_state(handle.0.id()),
                alpha_mode = ?material.map(Material::alpha_mode), base_color = ?material.map(|material| material.base_color),
                alpha_mode_finite = ?material.map(|material| match material.alpha_mode {
                    AlphaMode::Mask(cutoff) => cutoff.is_finite(),
                    _ => true,
                }),
                cull = ?material.map(|material| material.cull_mode), double_sided = ?material.map(|material| material.double_sided),
                depth_bias = ?material.map(|material| material.depth_bias),
                base_color_finite = ?material.map(|material| {
                    let color = material.base_color.to_linear();
                    color.red.is_finite() && color.green.is_finite() && color.blue.is_finite() && color.alpha.is_finite()
                }),
                depth_bias_finite = ?material.map(|material| material.depth_bias.is_finite()),
                base_texture = ?material.and_then(|material| material.base_color_texture.as_ref()).map(Handle::id),
                base_texture_loaded = ?material.and_then(|material| material.base_color_texture.as_ref())
                    .map(|texture| self.images.as_ref().is_some_and(|images| images.get(texture).is_some())),
                depth_pipeline_state = "not exposed by StandardMaterial",
                "animal preview standard CPU material state");
        }
        if let Some(handle) = draw.effect {
            let material = self
                .effect_materials
                .as_ref()
                .and_then(|assets| assets.get(&handle.0));
            info!(target: "openzt2_verification", material = ?handle.0.id(), loaded = material.is_some(),
                material_path = ?self.server.get_path(handle.0.id()),
                load_state = ?self.server.get_load_state(handle.0.id()), alpha_mode = ?material.map(Material::alpha_mode),
                opaque_blend_state = ?material.map(|material| material.has_opaque_blend_state()),
                fixed_transform_view = ?material.map(|material| material.fixed_transform_runtime_measurements().0),
                view_specific_registers = ?material.map(|material| material.requires_view_specific_shader_registers()),
                view_projection_provenance = "private owner; only draw hierarchy and fixed-transform view observed",
                cull_depth_and_color_write_state = "private pipeline specialization state; not observed",
                effect_texture_and_register_values = "not observed",
                "animal preview effect CPU material state");
        }
    }
}

fn log_vertex_color_summary(role: &'static str, colors: Option<&[[f32; 4]]>) {
    // Borrow all colors of each already-capped detailed draw. No vertex buffers
    // are copied; extrema exclude nonfinite channels and remain absent if empty.
    let Some(colors) = colors else {
        info!(target: "openzt2_verification", role, available = false,
            "animal preview vertex color summary");
        return;
    };
    let mut finite_channels = [0_usize; 4];
    let mut minimum = [None::<f32>; 4];
    let mut maximum = [None::<f32>; 4];
    let mut nonfinite_colors = 0_usize;
    for color in colors {
        nonfinite_colors += usize::from(color.iter().any(|value| !value.is_finite()));
        for (channel, value) in color.iter().copied().enumerate() {
            if value.is_finite() {
                finite_channels[channel] += 1;
                minimum[channel] = Some(minimum[channel].map_or(value, |old| old.min(value)));
                maximum[channel] = Some(maximum[channel].map_or(value, |old| old.max(value)));
            }
        }
    }
    info!(target: "openzt2_verification", role, available = true, count = colors.len(),
        nonfinite_colors, ?finite_channels, finite_min_rgba = ?minimum, finite_max_rgba = ?maximum,
        "animal preview vertex color summary");
}
