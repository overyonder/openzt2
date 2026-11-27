use super::*;
use crate::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;

#[test]
fn lowers_a_standalone_award_group_as_a_ui_fragment() {
    let path = "awards/earned.xml";
    let source = parse_blue_fang_source_document(
        AssetPath::new(path),
        br#"<UIListBox name="Awards" rows="-1" autosize="true"><UIRegion x="0" y="0" w="260" Doe="300"/></UIListBox>"#,
    )
    .unwrap();
    let documents = lower_selected_ui_source_documents_to_canonical_result_documents(
        &[source],
        UiResolvedDependencies::default(),
        path,
    )
    .unwrap();
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].role, UiDocumentRole::Fragment);
    assert_eq!(documents[0].nodes[0].name, "Awards");
}
