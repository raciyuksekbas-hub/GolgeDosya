use ekler_core::*;
#[test]
fn unsupported_schema_rejected() {
    let mut p = Project::new("test");
    p.version = "999".into();
    assert!(persistence::validate_project_structure(&p).is_err());
}
#[test]
fn delete_then_create_never_reuses_live_id() {
    let mut p = Project::new("test");
    p.create_exhibit("a");
    p.create_exhibit("b");
    p.remove_exhibit("exhibit-1");
    p.create_exhibit("c");
    assert_ne!(p.exhibits[0].id, p.exhibits[1].id);
}
#[test]
fn project_save_is_no_clobber_and_round_trips() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("İş Dosyası.duzenek");
    let p = Project::new("İş Dosyası");
    persistence::save_project(&p, &path).unwrap();
    assert_eq!(persistence::load_project(&path).unwrap().project, p);
    assert!(persistence::save_project(&p, &path).is_err());
}
#[test]
fn missing_modified_and_relinked_sources_are_reported() {
    let d = tempfile::tempdir().unwrap();
    let old = d.path().join("source.png");
    ::image::RgbImage::from_pixel(8, 8, ::image::Rgb([4, 5, 6]))
        .save(&old)
        .unwrap();
    let source = scan_source_files(&[&old]).sources.remove(0);
    let id = source.id.clone();
    let mut p = Project::new("relink");
    p.add_source(source);
    let saved = d.path().join("project.duzenek");
    persistence::save_project(&p, &saved).unwrap();
    let moved = d.path().join("taşınmış.png");
    std::fs::rename(&old, &moved).unwrap();
    assert_eq!(persistence::load_project(&saved).unwrap().issues.len(), 1);
    let report = persistence::relink_source(&p, &id, &moved).unwrap();
    assert!(report.issues.is_empty());
    std::fs::write(&moved, b"changed").unwrap();
    assert_eq!(persistence::source_issues(&report.project).len(), 1);
    assert!(persistence::relink_source(&p, &id, &moved).is_err());
}
