//! Opt-in proof that one live material reaches Bevy's render world.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use bevy::{
    app::AppExit,
    asset::{AssetId, AssetServer, Assets, Handle, UntypedAssetId},
    ecs::system::Commands,
    pbr::PreparedMaterial,
    prelude::*,
    render::{erased_render_asset::ErasedRenderAssets, renderer::RenderDevice, Render, RenderApp},
};

use super::{
    material_asset_types::MaterialAsset, runtime::effect_pass_gpu_data::EffectPassMaterial,
};
use crate::plugins::settings::graphics_settings_types::GraphicsSettings;

#[derive(Resource, Clone)]
struct MaterialRuntimeProofSignal {
    pass_material_asset_id: Arc<Mutex<Option<UntypedAssetId>>>,
    pass_material_is_prepared: Arc<AtomicBool>,
}

#[derive(Resource)]
struct MaterialRuntimeProofState {
    material_asset_path: String,
    material_asset: Handle<MaterialAsset>,
    scene_spawned: bool,
    prepared_render_frames: u32,
}

pub(super) fn install_material_runtime_render_world_proof(app: &mut App) {
    let Some(path) = std::env::var_os("OPENZT2_FX_RUNTIME_PROOF") else {
        return;
    };
    let signal = MaterialRuntimeProofSignal {
        pass_material_asset_id: Arc::new(Mutex::new(None)),
        pass_material_is_prepared: Arc::new(AtomicBool::new(false)),
    };
    app.insert_resource(signal.clone())
        .insert_resource(MaterialRuntimeProofState {
            material_asset_path: path.to_string_lossy().into_owned(),
            material_asset: Handle::default(),
            scene_spawned: false,
            prepared_render_frames: 0,
        })
        .add_systems(Startup, begin_loading_material_runtime_proof_asset)
        .add_systems(
            Update,
            spawn_material_runtime_proof_scene_and_exit_after_gpu_preparation,
        );
    let render_app = app
        .get_sub_app_mut(RenderApp)
        .expect("RenderPlugin must be installed before material plugins");
    render_app
        .insert_resource(signal)
        .add_systems(Render, observe_material_runtime_proof_pass_in_render_world);
}

fn begin_loading_material_runtime_proof_asset(
    server: Res<AssetServer>,
    mut proof: ResMut<MaterialRuntimeProofState>,
) {
    proof.material_asset = server.load(proof.material_asset_path.clone());
    info!(asset = %proof.material_asset_path, "FX runtime proof requested live material");
}

fn spawn_material_runtime_proof_scene_and_exit_after_gpu_preparation(
    mut commands: Commands,
    mut proof: ResMut<MaterialRuntimeProofState>,
    signal: Res<MaterialRuntimeProofSignal>,
    materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    passes: Res<Assets<EffectPassMaterial>>,
    images: Res<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut exits: MessageWriter<AppExit>,
) {
    if !proof.scene_spawned {
        let Some(material) = materials.get(&proof.material_asset) else {
            return;
        };
        let selected_passes =
            material.evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects);
        let Some(pass) = selected_passes.first() else {
            panic!("FX runtime proof material has no evaluated passes");
        };
        if passes.get(pass).is_none() {
            return;
        }
        let mut mesh = Mesh::from(Rectangle::new(2.0, 2.0));
        let vertices = mesh.count_vertices();
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0_f32; 2]; vertices]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0_f32; 4]; vertices]);
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d::<EffectPassMaterial>(pass.clone()),
            Transform::default(),
        ));
        commands.spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        *signal
            .pass_material_asset_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(AssetId::untyped(pass.id()));
        proof.scene_spawned = true;
        info!(
            asset = %proof.material_asset_path,
            techniques = material.evaluated_d3d9_effect().evaluated_techniques.len(),
            passes = selected_passes.len(),
            bound_textures = material.bound_texture_assets.len(),
            loaded_bound_textures = material
                .bound_texture_assets
                .iter()
                .filter(|texture| images.get(*texture).is_some())
                .count(),
            first_pass_textures = passes
                .get(pass)
                .map_or(0, EffectPassMaterial::present_texture_asset_count),
            "FX runtime proof attached evaluated pass to a Bevy mesh"
        );
        return;
    }
    if signal.pass_material_is_prepared.load(Ordering::Acquire) {
        proof.prepared_render_frames += 1;
        if proof.prepared_render_frames == 30 {
            info!(
                asset = %proof.material_asset_path,
                "FX runtime proof observed the pass material in Bevy's GPU render world"
            );
            exits.write(AppExit::Success);
        }
    }
}

fn observe_material_runtime_proof_pass_in_render_world(
    signal: Res<MaterialRuntimeProofSignal>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    _: Res<RenderDevice>,
) {
    let pass = *signal
        .pass_material_asset_id
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if pass.is_some_and(|pass| materials.get(pass).is_some()) {
        signal
            .pass_material_is_prepared
            .store(true, Ordering::Release);
    }
}
