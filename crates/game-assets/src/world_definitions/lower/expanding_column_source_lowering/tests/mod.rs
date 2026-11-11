use super::lower_expanding_column_presentation;
use crate::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
use crate::source_document::path::AssetPath;
use crate::source_document::resolved_source_record_index::SourceIndex;
use std::collections::BTreeMap;

fn support(key: &str, fit: &str) -> String {
    format!(
        r#"<BFTypedBinder binderType="{key}"><types><entity><scenery/></entity></types><binder>
<BFNamedBinder binderName="mainObj"><instance><BFPhysObj><BFSceneGraphComponent modelfile="base"/>{fit}</BFPhysObj></instance></BFNamedBinder>
<BFNamedBinder binderName="topObj_T"><instance><BFPhysObj><BFSceneGraphComponent modelfile="top"/></BFPhysObj></instance></BFNamedBinder>
<BFNamedBinder binderName="expandoPiece1"><instance><BFPhysObj><BFSceneGraphComponent modelfile="pole"/></BFPhysObj></instance></BFNamedBinder>
<BFBinder><instance><ZTExpandoComponent><Base binderName="mainObj" height="1.0f"/><Top><TopPiece1 binderName="topObj_T" height="1.0f"/></Top><Expando><Piece1 binderName="expandoPiece1" height="1.0f"/></Expando></ZTExpandoComponent></instance></BFBinder>
</binder></BFTypedBinder>"#
    )
}

#[test]
fn a_support_base_that_fits_lowest_keeps_its_fit_radius() -> Result<(), Box<dyn std::error::Error>>
{
    // Shipped elevated path supports author fitLowest with fitRadius 1.5.
    let documents = [
        (
            "entities/lowest.xml",
            support(
                "Lowest",
                r#"<BFGroundFitComponent heightOffset="0.5" fitLowest="true" fitRadius="1.5" />"#,
            ),
        ),
        (
            "entities/plain.xml",
            support("Plain", r#"<BFGroundFitComponent fitRadius="1.5" />"#),
        ),
    ]
    .map(|(path, source)| parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes()))
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let index = SourceIndex::build(&documents)?;
    let scenes = ["base", "top", "pole"]
        .map(|name| (name.to_owned(), format!("entities/{name}.nif")))
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let radius = |key| -> Result<f32, Box<dyn std::error::Error>> {
        let record = index.find(key).ok_or(key)?;
        Ok(
            lower_expanding_column_presentation(&record, "TopPiece1", &scenes)?
                .fit_lowest_radius_metres,
        )
    };
    assert!((radius("Lowest")? - 1.5).abs() < f32::EPSILON);
    assert!(radius("Plain")?.abs() < f32::EPSILON);
    Ok(())
}
