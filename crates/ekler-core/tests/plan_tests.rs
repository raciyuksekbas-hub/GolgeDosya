use ekler_core::*;
fn source(id: &str, pages: usize) -> SourceFile {
    SourceFile {
        id: id.into(),
        path: format!("{id}.pdf").into(),
        file_name: format!("{id}.pdf"),
        format: SourceFormat::Pdf,
        size_bytes: 0,
        sha256_before: String::new(),
        mtime: 0,
        page_count: pages,
        is_signed: false,
        signature_note: None,
        signed_policy: SignedPolicy::UseOriginalAsIs,
        is_approved_for_conversion: false,
        is_repaired: false,
        repair_note: None,
    }
}
#[test]
fn new_exhibit_split_preserves_source_ranges_and_renumbers() {
    let mut p = Project::new("test");
    p.add_source(source("a", 10));
    p.add_source(source("b", 10));
    p.create_exhibit("first").sources = vec![
        ExhibitSourceRef {
            source_id: "a".into(),
            page_range: Some((3, 10)),
        },
        ExhibitSourceRef {
            source_id: "b".into(),
            page_range: None,
        },
    ];
    p.create_exhibit("second");
    let changed = split_into_new_exhibit(&p, "exhibit-1", 9).unwrap();
    assert_eq!(
        changed.exhibits.iter().map(|e| e.order).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(changed.exhibits[0].sources[0].page_range, Some((3, 10)));
    assert_eq!(changed.exhibits[0].sources[1].page_range, Some((1, 1)));
    assert_eq!(changed.exhibits[1].sources[0].source_id, "b");
    assert_eq!(changed.exhibits[1].sources[0].page_range, Some((2, 10)));
    assert_eq!(p.exhibits.len(), 2);
    assert_eq!(changed, split_into_new_exhibit(&p, "exhibit-1", 9).unwrap());
}
#[test]
fn indivisible_signed_original_rejects_new_exhibit_split() {
    let mut p = Project::new("test");
    let mut s = source("a", 20);
    s.is_signed = true;
    p.add_source(s);
    p.create_exhibit("first").sources.push(ExhibitSourceRef {
        source_id: "a".into(),
        page_range: None,
    });
    assert!(split_into_new_exhibit(&p, "exhibit-1", 18).is_err());
}
