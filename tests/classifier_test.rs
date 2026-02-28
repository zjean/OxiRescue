use oxirescue::blob::classifier::{MimeCategory, classify_mime};

#[test]
fn test_classify_jpeg() {
    let head = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
    let (cat, ext) = classify_mime(head);
    assert_eq!(cat, MimeCategory::Images);
    assert_eq!(ext, "jpg");
}

#[test]
fn test_classify_pdf() {
    let head = b"%PDF-1.4 fake header";
    let (cat, ext) = classify_mime(head);
    assert_eq!(cat, MimeCategory::Documents);
    assert_eq!(ext, "pdf");
}

#[test]
fn test_classify_unknown() {
    let head = &[0x00, 0x01, 0x02, 0x03];
    let (cat, ext) = classify_mime(head);
    assert_eq!(cat, MimeCategory::Unknown);
    assert_eq!(ext, "bin");
}

#[test]
fn test_classify_png() {
    let head = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let (cat, ext) = classify_mime(head);
    assert_eq!(cat, MimeCategory::Images);
    assert_eq!(ext, "png");
}
