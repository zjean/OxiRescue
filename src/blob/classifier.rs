use infer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MimeCategory {
    Images,
    Documents,
    Video,
    Audio,
    Unknown,
}

impl MimeCategory {
    pub fn dir_name(&self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Documents => "documents",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for MimeCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.dir_name())
    }
}

/// Classify file content by its magic bytes.
/// Returns (category, file extension).
pub fn classify_mime(head: &[u8]) -> (MimeCategory, &'static str) {
    match infer::get(head) {
        Some(kind) => {
            let mime = kind.mime_type();
            let ext = kind.extension();
            let cat = if mime.starts_with("image/") {
                MimeCategory::Images
            } else if mime.starts_with("video/") {
                MimeCategory::Video
            } else if mime.starts_with("audio/") {
                MimeCategory::Audio
            } else if is_document_mime(mime) {
                MimeCategory::Documents
            } else {
                MimeCategory::Unknown
            };
            (cat, ext)
        }
        None => (MimeCategory::Unknown, "bin"),
    }
}

fn is_document_mime(mime: &str) -> bool {
    matches!(mime,
        "application/pdf"
        | "application/msword"
        | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        | "application/vnd.ms-excel"
        | "application/vnd.ms-powerpoint"
        | "application/vnd.oasis.opendocument.text"
        | "application/vnd.oasis.opendocument.spreadsheet"
        | "application/vnd.oasis.opendocument.presentation"
        | "application/rtf"
        | "application/epub+zip"
        | "text/plain"
        | "text/csv"
        | "text/html"
        | "text/xml"
        | "application/xml"
        | "application/json"
    )
}
