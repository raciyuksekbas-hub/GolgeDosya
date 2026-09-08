use ekler_core::*;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document as LopdfDoc, Object, Stream};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

fn make_pdf(pages: usize, blank_page_index: Option<usize>) -> tempfile::NamedTempFile {
    let mut doc = LopdfDoc::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut page_ids = Vec::new();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    for i in 1..=pages {
        let is_blank = blank_page_index == Some(i);
        let content = if is_blank {
            Content { operations: vec![] }
        } else {
            Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 12.into()]),
                    Operation::new("Td", vec![50.into(), 750.into()]),
                    Operation::new("Tj", vec![Object::string_literal(format!("Sayfa {}", i))]),
                    Operation::new("ET", vec![]),
                ],
            }
        };

        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.28.into(), 841.89.into()],
            "Resources" => dictionary! {
                "Font" => dictionary! {
                    "F1" => font_id,
                },
            },
        });
        page_ids.push(page_id);
    }

    let pages_obj = dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
        "Count" => page_ids.len() as i32,
    };
    doc.set_object(pages_id, pages_obj);

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let tmp = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
    let mut f = File::create(tmp.path()).unwrap();
    doc.save_to(&mut f).unwrap();
    f.flush().unwrap();
    tmp
}

#[test]
fn test_merge_and_extract_pages() {
    let pdf1 = make_pdf(2, None);
    let pdf2 = make_pdf(3, None);

    let dir = tempdir().unwrap();
    let merged_path = dir.path().join("merged.pdf");

    merge_pdf_files(
        &[pdf1.path().to_path_buf(), pdf2.path().to_path_buf()],
        &merged_path,
    )
    .unwrap();

    let info = pdf::inspect_pdf(&merged_path).unwrap();
    assert_eq!(info.page_count, 5);

    let extracted_path = dir.path().join("extracted.pdf");
    extract_pages_to_file(&merged_path, 2, 4, &extracted_path).unwrap();

    let ext_info = pdf::inspect_pdf(&extracted_path).unwrap();
    assert_eq!(ext_info.page_count, 3);
}

#[test]
fn test_delete_pages_and_rotation() {
    let pdf = make_pdf(4, None);
    let dir = tempdir().unwrap();

    let deleted_path = dir.path().join("deleted.pdf");
    delete_pages_from_file(pdf.path(), &[2, 3], &deleted_path).unwrap();

    let info = pdf::inspect_pdf(&deleted_path).unwrap();
    assert_eq!(info.page_count, 2);

    let rotated_path = dir.path().join("rotated.pdf");
    rotate_pdf_pages(&deleted_path, 90, &rotated_path).unwrap();
    let rot_doc = LopdfDoc::load(&rotated_path).unwrap();
    let p1 = rot_doc.get_pages().get(&1).cloned().unwrap();
    let page_dict = rot_doc.get_object(p1).unwrap().as_dict().unwrap();
    assert_eq!(page_dict.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
}

#[test]
fn test_detect_blank_pages() {
    // Sayfa 2 boş olan 3 sayfalık belge
    let pdf = make_pdf(3, Some(2));
    let doc = LopdfDoc::load(pdf.path()).unwrap();

    let blanks = detect_likely_blank_pages(&doc);
    assert_eq!(blanks, vec![2]);
}

#[test]
fn test_pdf_optimizer_low_risk() {
    let pdf = make_pdf(3, None);
    let mut doc = LopdfDoc::load(pdf.path()).unwrap();

    let res = optimize_pdf(&mut doc, OptimizationLevel::LowRiskCleanup).unwrap();
    assert!(res.optimized_size_bytes > 0);
}
