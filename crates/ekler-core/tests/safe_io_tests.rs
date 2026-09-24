use ekler_core::{calculate_sha256, safe_io::*};
#[test]
fn never_overwrite_existing_source_or_hardlink() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("source");
    std::fs::write(&src, b"original").unwrap();
    let alias = d.path().join("alias");
    std::fs::hard_link(&src, &alias).unwrap();
    let hash = calculate_sha256(&src).unwrap();
    for p in [&src, &alias] {
        assert!(write_new_bytes(p, std::slice::from_ref(&src), b"changed").is_err());
    }
    assert_eq!(hash, calculate_sha256(&src).unwrap());
}
#[cfg(unix)]
#[test]
fn symlinks_and_dangling_symlinks_are_not_overwritten() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("source");
    std::fs::write(&src, b"original").unwrap();
    for (name, target) in [
        ("alias", src.clone()),
        ("dangling", d.path().join("missing")),
    ] {
        let alias = d.path().join(name);
        std::os::unix::fs::symlink(target, &alias).unwrap();
        assert!(write_new_bytes(&alias, std::slice::from_ref(&src), b"bad").is_err());
    }
}
#[test]
fn failed_writer_leaves_no_final_file() {
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("result");
    assert!(write_new_file(&dest, &[], |f| {
        use std::io::Write;
        f.write_all(b"partial").unwrap();
        Err(ekler_core::EklerError::ValidationFailed(
            "injected failure".into(),
        ))
    })
    .is_err());
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
}
/// §55: Windows'un eski 260 karakterlik sınırını aşan klasörde yayın.
/// Windows'ta `tempfile`'ın yayını yolu Win32'ye öneksiz veriyordu ve bu
/// test "os error 3" ile düşüyordu; diğer sistemlerde davranışı sabitler.
#[test]
fn publication_works_beyond_the_windows_path_limit() {
    let d = tempfile::tempdir().unwrap();
    let mut deep = d
        .path()
        .join("Çağrı Şahin")
        .join("Müvekkil'in Dosyası (2026)");
    while deep.as_os_str().len() < 300 {
        deep = deep.join("Ayrıntılı alt klasör adı");
    }
    std::fs::create_dir_all(&deep).unwrap();
    let dest = deep.join("Ek listesi — Çağrı'nın kopyası.txt");
    write_new_bytes(&dest, &[], b"ek listesi").unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"ek listesi");
    assert!(write_new_bytes(&dest, &[], b"ikinci").is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), b"ek listesi");
    assert_eq!(
        std::fs::read_dir(&deep).unwrap().count(),
        1,
        "geçici dosya kaldı"
    );
}
#[test]
fn package_commit_collision_preserves_both_directories() {
    let d = tempfile::tempdir().unwrap();
    let staged = d.path().join("staged");
    let final_dir = d.path().join("final");
    std::fs::create_dir(&staged).unwrap();
    std::fs::create_dir(&final_dir).unwrap();
    for i in 1..=7 {
        std::fs::write(staged.join(format!("{i}.pdf")), b"prepared").unwrap();
    }
    assert!(publish_directory(&staged, &final_dir).is_err());
    assert_eq!(std::fs::read_dir(final_dir).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(staged).unwrap().count(), 7);
}

fn source_pdf(path: &std::path::Path) {
    use lopdf::{dictionary, Document};
    let mut d = Document::with_version("1.7");
    let pages = d.new_object_id();
    let page=d.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()]});
    d.set_object(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1},
    );
    let cat = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer.set("Root", cat);
    d.save(path).unwrap();
}
#[test]
fn toolbox_cannot_rotate_in_place() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("source.pdf");
    source_pdf(&src);
    let before = calculate_sha256(&src).unwrap();
    assert!(ekler_core::rotate_pdf_pages(&src, 90, &src).is_err());
    assert_eq!(before, calculate_sha256(&src).unwrap());
}
#[test]
fn pipeline_uses_new_package_even_with_same_source_filename() {
    use ekler_core::*;
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("EK-01_Belge_S001-S001.pdf");
    source_pdf(&src);
    let before = calculate_sha256(&src).unwrap();
    let source = scan_source_files(&[&src]).sources.remove(0);
    let id = source.id.clone();
    let mut p = Project::new("Paket");
    p.add_source(source);
    p.create_exhibit("Belge").sources.push(ExhibitSourceRef {
        source_id: id,
        page_range: None,
    });
    let result = execute_uyap_preparation(
        &p,
        &ExecutionContext {
            output_dir: d.path().into(),
        },
    )
    .unwrap();
    assert_ne!(result.package_dir, d.path());
    assert_eq!(before, calculate_sha256(&src).unwrap());
    assert!(result
        .package_dir
        .join(&result.outputs[0].file_name)
        .exists());
    let second = execute_uyap_preparation(
        &p,
        &ExecutionContext {
            output_dir: d.path().into(),
        },
    )
    .unwrap();
    assert_ne!(result.package_dir, second.package_dir);
}
#[test]
fn oversized_single_page_never_commits() {
    use ekler_core::*;
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("source.pdf");
    source_pdf(&src);
    let source = scan_source_files(&[&src]).sources.remove(0);
    let id = source.id.clone();
    let mut p = Project::new("Paket");
    p.add_source(source);
    p.create_exhibit("Belge").sources.push(ExhibitSourceRef {
        source_id: id,
        page_range: None,
    });
    p.target_size_bytes = 1;
    let out = d.path().join("output");
    assert!(execute_uyap_preparation(
        &p,
        &ExecutionContext {
            output_dir: out.clone()
        }
    )
    .is_err());
    assert_eq!(std::fs::read_dir(out).unwrap().count(), 0);
}
#[test]
fn signed_original_policy_and_plan_tampering_are_backend_invariants() {
    use ekler_core::*;
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("İmzalı örnek.pdf");
    source_pdf(&src);
    let mut doc = lopdf::Document::load(&src).unwrap();
    doc.add_object(lopdf::dictionary!{"Type"=>"Sig","ByteRange"=>vec![0.into(),1.into(),2.into(),3.into()],"Contents"=>lopdf::Object::string_literal("synthetic")});
    doc.save(&src).unwrap();
    let scan = scan_source_files(&[&src]);
    let s = scan.sources[0].clone();
    assert!(s.is_signed);
    let mut p = Project::new("signed");
    p.add_source(s.clone());
    p.create_exhibit("signed").sources.push(ExhibitSourceRef {
        source_id: s.id.clone(),
        page_range: None,
    });
    let plan = prepare_export_plan(&p).unwrap();
    let out = execute_export_plan(
        &plan,
        &ExecutionContext {
            output_dir: d.path().into(),
        },
    )
    .unwrap();
    assert_eq!(out.outputs[0].page_count, 1);
    assert!(out.outputs[0].file_name.ends_with(".pdf"));
    assert_eq!(
        std::fs::read(&src).unwrap(),
        std::fs::read(out.package_dir.join(&out.outputs[0].file_name)).unwrap()
    );
    let mut changed = plan.clone();
    changed.outputs[0].page_end = 2;
    assert!(execute_export_plan(
        &changed,
        &ExecutionContext {
            output_dir: d.path().into()
        }
    )
    .is_err());
    p.sources[0].signed_policy = SignedPolicy::CreateDerivedCopy;
    assert!(prepare_export_plan(&p).is_err());
    p.sources[0].is_approved_for_conversion = true;
    assert!(prepare_export_plan(&p).is_ok());
    p.sources[0].is_signed = false;
    assert!(prepare_export_plan(&p).is_err());
}
#[test]
fn toolbox_reorder_validation_and_new_operations_preserve_source() {
    use ekler_core::toolbox::*;
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source.pdf");
    source_pdf(&source);
    let before = std::fs::read(&source).unwrap();
    for (i, op) in [
        ToolOperation::Select { pages: vec![1] },
        ToolOperation::Reorder { pages: vec![1] },
        ToolOperation::Rotate { degrees: 90 },
        ToolOperation::Compress {
            level: ekler_core::OptimizationLevel::GentleCompression,
        },
        ToolOperation::Crop { margin_pt: 20. },
        ToolOperation::Watermark {
            text: "KOPYA".into(),
        },
        ToolOperation::Number { start: 7 },
    ]
    .iter()
    .enumerate()
    {
        let out = d.path().join(format!("output-{i}.pdf"));
        if matches!(op, ToolOperation::Compress { .. }) {
            assert!(matches!(
                run_tool_with_outcome(std::slice::from_ref(&source), op, &out, false).unwrap(),
                ToolOutcome::NoBenefit { .. }
            ));
            assert!(!out.exists());
        } else {
            run_tool(std::slice::from_ref(&source), op, &out, false).unwrap();
            assert_eq!(lopdf::Document::load(out).unwrap().get_pages().len(), 1);
        }
    }
    assert!(run_tool(
        std::slice::from_ref(&source),
        &ToolOperation::Reorder { pages: vec![1, 1] },
        &d.path().join("invalid.pdf"),
        false
    )
    .is_err());
    assert!(!d.path().join("invalid.pdf").exists());
    assert_eq!(before, std::fs::read(&source).unwrap());
}

#[test]
fn fourth_staged_write_failure_never_publishes_a_partial_package() {
    let root = tempfile::tempdir().unwrap();
    let final_dir = root.path().join("final-package");
    let result = (|| -> ekler_core::Result<()> {
        let staged = tempfile::Builder::new()
            .prefix(".duzenek-export-")
            .suffix(".tmp")
            .tempdir_in(root.path())
            .unwrap();
        for index in 1..=4 {
            write_new_file(&staged.path().join(format!("{index}.pdf")), &[], |file| {
                use std::io::Write;
                file.write_all(b"synthetic staged bytes").unwrap();
                if index == 4 {
                    return Err(ekler_core::EklerError::ValidationFailed(
                        "Injected fourth writer failure".into(),
                    ));
                }
                Ok(())
            })?;
        }
        publish_directory(staged.path(), &final_dir)
    })();
    assert!(result.is_err());
    assert!(!final_dir.exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
#[cfg(unix)]
fn denied_destination_permission_is_an_error_without_output() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("locked");
    std::fs::create_dir(&destination).unwrap();
    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = write_new_bytes(&destination.join("new.pdf"), &[], b"new bytes");
    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        result.is_err(),
        "Run the permission gate as an ordinary user, not root"
    );
    assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 0);
}
