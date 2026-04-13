use actix_multipart::form::{tempfile::TempFile, text::Text, MultipartForm};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Query {
    pub path: String,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub path: String,
    pub filter: Option<String>,
    pub deep: Option<bool>,
    pub size: Option<String>,
}

#[derive(Deserialize)]
pub struct NewFolderRequest {
    pub name: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct NewFileRequest {
    pub name: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub path: String,
    pub item: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct MoveRequest {
    pub path: String,
    pub sources: Vec<String>,
    pub destination: String,
}

#[derive(Deserialize)]
pub struct CopyRequest {
    pub path: String,
    pub sources: Vec<String>,
    pub destination: String,
}

#[derive(Deserialize)]
pub struct DeleteRequest {
    pub path: String,
    pub items: Vec<FileItem>,
}

#[derive(Deserialize)]
pub struct ArchiveRequest {
    pub name: String,
    pub items: Vec<FileItem>,
    pub path: String,
}

#[derive(Deserialize)]
pub struct UnarchiveRequest {
    pub item: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct SaveRequest {
    pub path: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct FileItem {
    pub path: String,
    pub r#type: String,
}

#[derive(Debug, MultipartForm)]
pub struct UploadForm {
    #[multipart(rename = "path")]
    pub path: Text<String>,

    #[multipart(rename = "name")]
    pub name: Text<String>,

    #[multipart(rename = "file")]
    pub file: TempFile,
}
