use super::lower_authored_presentation_physics;
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    resolved_source_record_index::SourceIndex,
};
use openzt2_game_data::AssetId;

#[test]
fn inherited_physics_stays_with_its_presentation_binder() -> Result<(), Box<dyn std::error::Error>>
{
    let base = parse_blue_fang_source_document(
        AssetPath::new("base.xml"),
        br#"
        <BFTypedBinder binderType="base"><types><base/></types><binder>
          <BFNamedBinder binderName="mainObj"><shared>
            <BFSharedPhysVars noCaustics="true" splashScale="2"/>
          </shared></BFNamedBinder>
        </binder></BFTypedBinder>"#,
    )?;
    let child = parse_blue_fang_source_document(
        AssetPath::new("child.xml"),
        br#"
        <BFTypedBinder binderType="child"><types><base/><child/></types><binder>
          <BFNamedBinder binderName="mainObj"><shared><BFSharedPhysVars splashScale="3"/></shared>
            <BFNamedBinder binderName="decal"><instance><BFPhysObj>
              <BFTerrainDecalComponent/><BFSharedPhysVars noCaustics="false" splashScale="9"/>
            </BFPhysObj></instance></BFNamedBinder>
          </BFNamedBinder>
          <BFNamedBinder binderName="other"><instance><BFPhysObj/></instance></BFNamedBinder>
        </binder></BFTypedBinder>"#,
    )?;
    let direct = parse_blue_fang_source_document(
        AssetPath::new("direct.xml"),
        br#"
        <BFTypedBinder binderType="direct" model="direct.nif"/>"#,
    )?;
    let index = SourceIndex::build([&base, &child, &direct])?;
    let child = index.find("child").ok_or("child source record missing")?;
    let (policies, strength) = lower_authored_presentation_physics(&child)?;
    assert_eq!(strength, Some(3.0));
    let policy = |name| {
        policies
            .iter()
            .find(|policy| policy.binder == AssetId::from_key(name))
    };
    let main = policy("mainobj").ok_or("main policy missing")?;
    assert!(main.no_caustics);
    assert!(!main.is_decal);
    let decal = policy("decal").ok_or("decal policy missing")?;
    assert!(!decal.no_caustics);
    assert!(decal.is_decal);
    let other = policy("other").ok_or("other policy missing")?;
    assert!(!other.no_caustics && !other.is_decal);
    let direct = index.find("direct").ok_or("direct source record missing")?;
    let (policies, strength) = lower_authored_presentation_physics(&direct)?;
    assert_eq!(strength, None);
    assert_eq!(policies.len(), 1);
    assert_eq!(policies[0].binder, AssetId::from_key("mainobj"));
    assert!(!policies[0].no_caustics && !policies[0].is_decal);
    Ok(())
}
