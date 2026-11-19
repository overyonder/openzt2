use bevy::prelude::*;
use openzt2_game_data::scene_prefab::PrefabLod;
use openzt2_world_state::{
    model_render::prefab_render_lod_tier::{
        PrefabRenderLodSelection, PrefabRenderLodTier, PrefabRenderLodVariant,
        PrefabRenderLodVariants,
    },
    world_spawn::prefab_model_level_of_detail_visibility_range::PrefabModelLevelOfDetailVisibilityRange,
};

pub(crate) fn bind_prefab_lod(
    commands: &mut Commands,
    entities: &[Entity],
    entity: Entity,
    lod: &PrefabLod,
) {
    let group = entities[lod.group_entity as usize];
    if let Some(initial) = lod.initial_render_tier {
        commands
            .entity(group)
            .insert_if_new(PrefabRenderLodTier(initial));
        commands.entity(entity).insert((
            PrefabRenderLodVariant {
                group,
                ordinal: lod.ordinal,
            },
            PrefabRenderLodSelection(initial == lod.ordinal),
        ));
    } else {
        commands
            .entity(entity)
            .insert(PrefabModelLevelOfDetailVisibilityRange {
                group,
                ordinal: lod.ordinal,
                center_m: Vec3::from_array(lod.center_m),
                near_m: lod.near_m,
                far_m: lod.far_m,
                active_without_range: lod.active_without_range,
            });
    }
}

pub(super) fn project_prefab_render_lod_tier(
    groups: Query<
        (&PrefabRenderLodTier, &PrefabRenderLodVariants),
        Or<(
            Changed<PrefabRenderLodTier>,
            Changed<PrefabRenderLodVariants>,
        )>,
    >,
    mut variants: Query<(
        &PrefabRenderLodVariant,
        &mut PrefabRenderLodSelection,
        &mut Visibility,
        Has<PrefabModelLevelOfDetailVisibilityRange>,
    )>,
) {
    for (selected, members) in &groups {
        for entity in members.iter() {
            let Ok((variant, mut selection, mut visibility, has_distance_range)) =
                variants.get_mut(entity)
            else {
                continue;
            };
            let shown = selected.0 == variant.ordinal;
            selection.set_if_neq(PrefabRenderLodSelection(shown));
            if !has_distance_range {
                visibility.set_if_neq(if shown {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}
