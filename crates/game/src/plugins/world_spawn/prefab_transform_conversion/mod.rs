use bevy::prelude::*;
use openzt2_game_data::scene_prefab::PrefabTransform as AuthoredPrefabTransform;

pub(crate) fn transform_from_authored(value: &AuthoredPrefabTransform) -> Transform {
    Transform {
        translation: Vec3::from_array(value.translation_m),
        rotation: Quat::from_array(value.rotation_xyzw),
        scale: Vec3::from_array(value.scale),
    }
}
