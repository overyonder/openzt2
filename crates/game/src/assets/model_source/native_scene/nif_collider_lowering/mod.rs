//! NetImmerse collision-volume lowering into prefab colliders.

use openzt2_game_data::scene_prefab::{PrefabColliderSource, PrefabTransform};

use crate::assets::source_coordinate_conversion::{
    conjugate_source_z_up_rotation_into_bevy_y_up_basis,
    convert_row_major_rotation_matrix_to_xyzw_quaternion,
    convert_source_z_up_vector_to_bevy_y_up_coordinates,
};

use super::super::netimmerse_nif_source::collision_source_types::NetImmerseBoundingVolume;
use super::{
    native_scene_lowering_error::NativeSceneLoweringError,
    scene_prefab_document_assembly::ScenePrefabDocumentAssembler,
    scene_transform_and_collider_math::{
        rotation_from_source_positive_y_axis, translated_prefab_transform,
    },
};

type Result<T> = std::result::Result<T, NativeSceneLoweringError>;

pub(super) fn lower_netimmerse_collision_volume(
    builder: &mut ScenePrefabDocumentAssembler,
    owner: u32,
    volume: &NetImmerseBoundingVolume,
    source_path: &str,
) -> Result<()> {
    match volume {
        NetImmerseBoundingVolume::Sphere(sphere) => {
            let entity = builder.anonymous_child(
                owner,
                translated_prefab_transform(convert_source_z_up_vector_to_bevy_y_up_coordinates(
                    sphere.center,
                )),
            )?;
            builder.collider(
                entity,
                PrefabColliderSource::Capsule {
                    radius_m: sphere.radius,
                    half_height_m: 0.0,
                },
            );
        }
        NetImmerseBoundingVolume::Box(box_) if box_.axes.len() == 3 => {
            let rows = [box_.axes[0], box_.axes[1], box_.axes[2]];
            let entity = builder.anonymous_child(
                owner,
                PrefabTransform {
                    translation_m: convert_source_z_up_vector_to_bevy_y_up_coordinates(box_.center),
                    rotation_xyzw: convert_row_major_rotation_matrix_to_xyzw_quaternion(
                        conjugate_source_z_up_rotation_into_bevy_y_up_basis(
                            rows.into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .try_into()
                                .unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
                        ),
                    ),
                    scale: [1.0; 3],
                },
            )?;
            builder.collider(
                entity,
                PrefabColliderSource::Box {
                    half_extent_m: [box_.extent[0], box_.extent[2], box_.extent[1]],
                },
            );
        }
        NetImmerseBoundingVolume::Capsule(capsule) => {
            let axis = convert_source_z_up_vector_to_bevy_y_up_coordinates(capsule.origin);
            let entity = builder.anonymous_child(
                owner,
                PrefabTransform {
                    translation_m: convert_source_z_up_vector_to_bevy_y_up_coordinates(
                        capsule.center,
                    ),
                    rotation_xyzw: rotation_from_source_positive_y_axis(axis),
                    scale: [1.0; 3],
                },
            )?;
            builder.collider(
                entity,
                PrefabColliderSource::Capsule {
                    radius_m: capsule.radius,
                    half_height_m: capsule.extent * 0.5,
                },
            );
        }
        NetImmerseBoundingVolume::Union(children) => {
            for child in children {
                lower_netimmerse_collision_volume(builder, owner, child, source_path)?;
            }
        }
        NetImmerseBoundingVolume::HalfSpace(_) => {
            return Err(NativeSceneLoweringError::new(
                source_path,
                "NIF half-space collision volume has no finite Bevy collider representation",
            ));
        }
        NetImmerseBoundingVolume::Unknown(kind) => {
            return Err(NativeSceneLoweringError::new(
                source_path,
                format!("unsupported NIF collision volume kind {kind}"),
            ));
        }
        NetImmerseBoundingVolume::Box(box_) => {
            return Err(NativeSceneLoweringError::new(
                source_path,
                format!(
                    "NIF box collision volume has {} axes instead of 3",
                    box_.axes.len()
                ),
            ));
        }
    }
    Ok(())
}
