//! Focused parsing of NIF particle geometry, controllers, modifiers, and colliders.

use super::{
    super::native_source_byte_reading::{
        read_f32_little_endian, read_f32_values, read_four_component_colour,
        read_i32_little_endian, read_source_boolean, read_three_component_vector,
        read_u16_little_endian, read_u32_little_endian, read_u8,
    },
    animation_controller_source_parsing::parse_time_controller,
    collision_and_skin_source_reading::read_plane,
    counted_source_collection_reading::read_i32_values,
    geometry_data_source_parsing::parse_geometry_data,
    particle_source_types::{
        NetImmerseNiGravity, NetImmerseNiParticleCollider, NetImmerseNiParticleColorModifier,
        NetImmerseNiParticleGrowFade, NetImmerseNiParticleInfo, NetImmerseNiParticleMeshModifier,
        NetImmerseNiParticleMeshesData, NetImmerseNiParticleModifier, NetImmerseNiParticleRotation,
        NetImmerseNiParticleSystemController, NetImmerseNiParticlesData,
        NetImmerseNiPlanarCollider,
    },
    source_error::NetImmerseNifSourceError,
};

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

pub(super) fn parse_particles_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticlesData> {
    let geometry = parse_geometry_data(source_bytes, cursor, source_path)?;
    let particle_radius = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticlesData particle radius",
    )?;
    let num_active = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticlesData active count",
    )?;
    let sizes = read_source_boolean(
        source_bytes,
        cursor,
        source_path,
        "NiParticlesData has sizes",
    )?
    .then(|| {
        read_f32_values(
            source_bytes,
            cursor,
            source_path,
            geometry.num_vertices,
            "NiParticlesData size",
        )
        .map_err(NetImmerseNifSourceError::from)
    })
    .transpose()?;
    let rotations = read_source_boolean(
        source_bytes,
        cursor,
        source_path,
        "NiParticlesData has rotations",
    )?
    .then(|| {
        (0..geometry.num_vertices)
            .map(|_| {
                read_four_component_colour(
                    source_bytes,
                    cursor,
                    source_path,
                    "NiParticlesData rotation",
                )
                .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()
    })
    .transpose()?;
    Ok(NetImmerseNiParticlesData {
        geometry,
        particle_radius,
        num_active,
        sizes,
        rotations,
    })
}

pub(super) fn parse_particle_meshes_data(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleMeshesData> {
    Ok(NetImmerseNiParticleMeshesData {
        particles: parse_particles_data(source_bytes, cursor, source_path)?,
        container_node_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleMeshesData container",
        )?,
    })
}

fn parse_particle_info(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleInfo> {
    Ok(NetImmerseNiParticleInfo {
        velocity: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiParticleInfo velocity",
        )?,
        unknown_vector: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiParticleInfo unknown vector",
        )?,
        lifetime: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleInfo lifetime",
        )?,
        lifespan: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleInfo lifespan",
        )?,
        timestamp: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleInfo timestamp",
        )?,
        unknown_short: read_u16_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleInfo unknown short",
        )?,
        vertex_id: read_u16_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleInfo vertex id",
        )?,
    })
}

pub(super) fn parse_particle_system_controller(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleSystemController> {
    let controller = parse_time_controller(source_bytes, cursor, source_path)?;
    let speed = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController speed",
    )?;
    let speed_random = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController speed random",
    )?;
    let vertical_direction = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController vertical direction",
    )?;
    let vertical_angle = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController vertical angle",
    )?;
    let horizontal_direction = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController horizontal direction",
    )?;
    let horizontal_angle = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController horizontal angle",
    )?;
    let initial_normal = read_three_component_vector(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController normal",
    )?;
    let initial_color = read_four_component_colour(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController color",
    )?;
    let size = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController size",
    )?;
    let emit_start_time = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController emit start",
    )?;
    let emit_stop_time = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController emit stop",
    )?;
    let unknown_byte = read_u8(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController unknown byte",
    )?;
    let emit_rate = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController emit rate",
    )?;
    let lifetime = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController lifetime",
    )?;
    let lifetime_random = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController lifetime random",
    )?;
    let emit_flags = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController emit flags",
    )?;
    let start_random = read_three_component_vector(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController start random",
    )?;
    let emitter_ref = read_i32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController emitter ref",
    )?;
    let unknown_short_2 = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController unknown short 2",
    )?;
    let unknown_float_13 = read_f32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController unknown float 13",
    )?;
    let unknown_int_1 = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController unknown int 1",
    )?;
    let unknown_int_2 = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController unknown int 2",
    )?;
    let unknown_short_3 = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController unknown short 3",
    )?;
    let particle_count = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController particle count",
    )?;
    let num_valid = read_u16_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleSystemController valid count",
    )?;
    let particles = (0..particle_count)
        .map(|_| parse_particle_info(source_bytes, cursor, source_path))
        .collect::<Result<Vec<_>>>()?;
    Ok(NetImmerseNiParticleSystemController {
        controller,
        speed,
        speed_random,
        vertical_direction,
        vertical_angle,
        horizontal_direction,
        horizontal_angle,
        initial_normal,
        initial_color,
        size,
        emit_start_time,
        emit_stop_time,
        unknown_byte,
        emit_rate,
        lifetime,
        lifetime_random,
        emit_flags,
        start_random,
        emitter_ref,
        unknown_short_2,
        unknown_float_13,
        unknown_int_1,
        unknown_int_2,
        unknown_short_3,
        num_valid,
        particles,
        unknown_link_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleSystemController unknown link",
        )?,
        particle_extra_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleSystemController particle extra",
        )?,
        unknown_link_2_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleSystemController unknown link 2",
        )?,
        trailer: read_u8(
            source_bytes,
            cursor,
            source_path,
            "NiParticleSystemController trailer",
        )?,
    })
}

fn parse_particle_modifier(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleModifier> {
    Ok(NetImmerseNiParticleModifier {
        next_modifier_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleModifier next ref",
        )?,
        controller_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleModifier controller ref",
        )?,
    })
}

pub(super) fn parse_particle_rotation(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleRotation> {
    Ok(NetImmerseNiParticleRotation {
        modifier: parse_particle_modifier(source_bytes, cursor, source_path)?,
        random_initial_axis: read_u8(
            source_bytes,
            cursor,
            source_path,
            "NiParticleRotation random axis",
        )?,
        initial_axis: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiParticleRotation initial axis",
        )?,
        rotation_speed: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleRotation speed",
        )?,
    })
}

pub(super) fn parse_particle_colour_modifier(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleColorModifier> {
    Ok(NetImmerseNiParticleColorModifier {
        modifier: parse_particle_modifier(source_bytes, cursor, source_path)?,
        color_data_ref: read_i32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleColorModifier color ref",
        )?,
    })
}

pub(super) fn parse_particle_growth_and_fade(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleGrowFade> {
    Ok(NetImmerseNiParticleGrowFade {
        modifier: parse_particle_modifier(source_bytes, cursor, source_path)?,
        grow: read_f32_little_endian(source_bytes, cursor, source_path, "NiParticleGrowFade grow")?,
        fade: read_f32_little_endian(source_bytes, cursor, source_path, "NiParticleGrowFade fade")?,
    })
}

pub(super) fn parse_particle_mesh_modifier(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleMeshModifier> {
    let modifier = parse_particle_modifier(source_bytes, cursor, source_path)?;
    let mesh_reference_count = read_u32_little_endian(
        source_bytes,
        cursor,
        source_path,
        "NiParticleMeshModifier mesh count",
    )?;
    Ok(NetImmerseNiParticleMeshModifier {
        modifier,
        particle_mesh_refs: read_i32_values(
            source_bytes,
            cursor,
            source_path,
            mesh_reference_count,
            "NiParticleMeshModifier mesh ref",
        )?,
    })
}

pub(super) fn parse_gravity_modifier(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiGravity> {
    Ok(NetImmerseNiGravity {
        modifier: parse_particle_modifier(source_bytes, cursor, source_path)?,
        decay: read_f32_little_endian(source_bytes, cursor, source_path, "NiGravity decay")?,
        force: read_f32_little_endian(source_bytes, cursor, source_path, "NiGravity force")?,
        field_type: read_u32_little_endian(source_bytes, cursor, source_path, "NiGravity type")?,
        position: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiGravity position",
        )?,
        direction: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiGravity direction",
        )?,
    })
}

fn parse_particle_collider(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiParticleCollider> {
    Ok(NetImmerseNiParticleCollider {
        modifier: parse_particle_modifier(source_bytes, cursor, source_path)?,
        bounce: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiParticleCollider bounce",
        )?,
        spawn_on_collide: read_source_boolean(
            source_bytes,
            cursor,
            source_path,
            "NiParticleCollider spawn flag",
        )?,
        die_on_collide: read_source_boolean(
            source_bytes,
            cursor,
            source_path,
            "NiParticleCollider die flag",
        )?,
    })
}

pub(super) fn parse_planar_particle_collider(
    source_bytes: &[u8],
    cursor: &mut usize,
    source_path: &str,
) -> Result<NetImmerseNiPlanarCollider> {
    Ok(NetImmerseNiPlanarCollider {
        collider: parse_particle_collider(source_bytes, cursor, source_path)?,
        height: read_f32_little_endian(
            source_bytes,
            cursor,
            source_path,
            "NiPlanarCollider height",
        )?,
        width: read_f32_little_endian(source_bytes, cursor, source_path, "NiPlanarCollider width")?,
        position: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiPlanarCollider position",
        )?,
        x_axis: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiPlanarCollider x vector",
        )?,
        y_axis: read_three_component_vector(
            source_bytes,
            cursor,
            source_path,
            "NiPlanarCollider y vector",
        )?,
        plane: read_plane(source_bytes, cursor, source_path, "NiPlanarCollider plane")?,
    })
}
