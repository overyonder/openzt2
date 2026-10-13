//! Exhaustive parsed NetImmerse NIF block payload and shared graph queries.

use super::{
    animation_controller_source_types::{
        NetImmerseNiAlphaController, NetImmerseNiBoneLodController, NetImmerseNiControllerSequence,
        NetImmerseNiGeomMorpherController, NetImmerseNiKeyframeController,
        NetImmerseNiKeyframeData, NetImmerseNiMaterialColorController, NetImmerseNiMorphData,
        NetImmerseNiTextKeyExtraData, NetImmerseNiUvController, NetImmerseNiVisController,
    },
    collision_source_types::NetImmerseNiCollisionData,
    geometry_data_source_types::{NetImmerseNiTriShapeData, NetImmerseNiTriStripsData},
    interpolated_key_source_types::{
        NetImmerseNiColorData, NetImmerseNiFloatData, NetImmerseNiPosData, NetImmerseNiUvData,
        NetImmerseNiVisData,
    },
    particle_source_types::{
        NetImmerseNiGravity, NetImmerseNiParticleColorModifier, NetImmerseNiParticleGrowFade,
        NetImmerseNiParticleMeshModifier, NetImmerseNiParticleMeshesData,
        NetImmerseNiParticleRotation, NetImmerseNiParticleSystemController,
        NetImmerseNiParticlesData, NetImmerseNiPlanarCollider,
    },
    render_property_and_texture_source_types::{
        NetImmerseNiAlphaProperty, NetImmerseNiBooleanExtraData, NetImmerseNiFlagProperty,
        NetImmerseNiIntegerExtraData, NetImmerseNiMaterialProperty, NetImmerseNiPixelData,
        NetImmerseNiSourceTexture, NetImmerseNiSpecularProperty, NetImmerseNiStencilProperty,
        NetImmerseNiStringExtraData, NetImmerseNiTextureEffect, NetImmerseNiTexturingProperty,
        NetImmerseNiVertexColorProperty, NetImmerseNiZBufferProperty,
    },
    scene_object_source_types::{
        NetImmerseNiAmbientLight, NetImmerseNiAvObject, NetImmerseNiBillboardNode,
        NetImmerseNiDirectionalLight, NetImmerseNiLodNode, NetImmerseNiNode,
        NetImmerseNiParticleMeshes, NetImmerseNiParticles, NetImmerseNiPointLight,
        NetImmerseNiTriShape, NetImmerseNiTriStrips,
    },
    transform_and_skin_source_types::{
        NetImmerseNiSkinData, NetImmerseNiSkinInstance, NetImmerseNiSkinPartition,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) enum NetImmerseNifBlockPayload {
    NiControllerSequence(NetImmerseNiControllerSequence),
    NiNode(NetImmerseNiNode),
    NiBillboardNode(NetImmerseNiBillboardNode),
    NiLODNode(NetImmerseNiLodNode),
    NiAmbientLight(NetImmerseNiAmbientLight),
    NiDirectionalLight(NetImmerseNiDirectionalLight),
    NiPointLight(NetImmerseNiPointLight),
    NiZBufferProperty(NetImmerseNiZBufferProperty),
    NiVertexColorProperty(NetImmerseNiVertexColorProperty),
    NiStringExtraData(NetImmerseNiStringExtraData),
    NiTriStrips(NetImmerseNiTriStrips),
    NiTriShape(NetImmerseNiTriShape),
    NiParticleMeshes(NetImmerseNiParticleMeshes),
    NiIntegerExtraData(NetImmerseNiIntegerExtraData),
    NiBooleanExtraData(NetImmerseNiBooleanExtraData),
    NiParticles(NetImmerseNiParticles),
    NiParticleSystemController(NetImmerseNiParticleSystemController),
    NiParticleRotation(NetImmerseNiParticleRotation),
    NiParticleColorModifier(NetImmerseNiParticleColorModifier),
    NiParticleGrowFade(NetImmerseNiParticleGrowFade),
    NiParticleMeshModifier(NetImmerseNiParticleMeshModifier),
    NiGravity(NetImmerseNiGravity),
    NiPlanarCollider(NetImmerseNiPlanarCollider),
    NiColorData(NetImmerseNiColorData),
    NiTexturingProperty(NetImmerseNiTexturingProperty),
    NiSourceTexture(NetImmerseNiSourceTexture),
    NiAlphaProperty(NetImmerseNiAlphaProperty),
    NiMaterialProperty(NetImmerseNiMaterialProperty),
    NiSpecularProperty(NetImmerseNiSpecularProperty),
    NiStencilProperty(NetImmerseNiStencilProperty),
    NiDitherProperty(NetImmerseNiFlagProperty),
    NiShadeProperty(NetImmerseNiFlagProperty),
    NiWireframeProperty(NetImmerseNiFlagProperty),
    NiTriStripsData(NetImmerseNiTriStripsData),
    NiTriShapeData(NetImmerseNiTriShapeData),
    NiParticlesData(NetImmerseNiParticlesData),
    NiParticleMeshesData(NetImmerseNiParticleMeshesData),
    NiAlphaController(NetImmerseNiAlphaController),
    NiKeyframeController(NetImmerseNiKeyframeController),
    NiMaterialColorController(NetImmerseNiMaterialColorController),
    NiUVController(NetImmerseNiUvController),
    NiGeomMorpherController(NetImmerseNiGeomMorpherController),
    NiVisController(NetImmerseNiVisController),
    NiKeyframeData(NetImmerseNiKeyframeData),
    NiFloatData(NetImmerseNiFloatData),
    NiPosData(NetImmerseNiPosData),
    NiUVData(NetImmerseNiUvData),
    NiVisData(NetImmerseNiVisData),
    NiMorphData(NetImmerseNiMorphData),
    NiCollisionData(NetImmerseNiCollisionData),
    NiSkinData(NetImmerseNiSkinData),
    NiSkinInstance(NetImmerseNiSkinInstance),
    NiSkinPartition(NetImmerseNiSkinPartition),
    NiTextKeyExtraData(NetImmerseNiTextKeyExtraData),
    NiTextureEffect(NetImmerseNiTextureEffect),
    NiPixelData(NetImmerseNiPixelData),
    NiBoneLODController(NetImmerseNiBoneLodController),
}
impl NetImmerseNifBlockPayload {
    /// The common scene object header carried by every transformable NIF block.
    pub(in super::super) const fn av_object(&self) -> Option<&NetImmerseNiAvObject> {
        match self {
            Self::NiNode(value) => Some(&value.av_object),
            Self::NiBillboardNode(value) => Some(&value.node.av_object),
            Self::NiLODNode(value) => Some(&value.switch_node.node.av_object),
            Self::NiAmbientLight(value) => Some(&value.light.av_object),
            Self::NiDirectionalLight(value) => Some(&value.light.av_object),
            Self::NiPointLight(value) => Some(&value.light.av_object),
            Self::NiTriShape(value) => Some(&value.geometry.av_object),
            Self::NiTriStrips(value) => Some(&value.geometry.av_object),
            Self::NiParticles(value) => Some(&value.geometry.av_object),
            Self::NiParticleMeshes(value) => Some(&value.geometry.av_object),
            Self::NiTextureEffect(value) => Some(&value.av_object),
            _ => None,
        }
    }

    /// Child and attached-effect references owned by NIF grouping nodes.
    pub(in super::super) fn child_and_effect_refs(&self) -> (&[i32], &[i32]) {
        match self {
            Self::NiNode(value) => (&value.child_refs, &value.effect_refs),
            Self::NiBillboardNode(value) => (&value.node.child_refs, &value.node.effect_refs),
            Self::NiLODNode(value) => (
                &value.switch_node.node.child_refs,
                &value.switch_node.node.effect_refs,
            ),
            _ => (&[], &[]),
        }
    }
}
