use actix_multipart::form::{tempfile::TempFile, text::Text, MultipartForm};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Query {
    #[serde(default)]
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
    #[serde(default)]
    pub items: Vec<FileItem>,
    #[serde(default)]
    pub sources: Vec<String>,
    pub destination: String,
}

impl MoveRequest {
    pub fn resolve_items(&self) -> Vec<FileItem> {
        if !self.items.is_empty() {
            return self.items.clone();
        }
        self.sources
            .iter()
            .map(|s| FileItem {
                path: s.clone(),
                r#type: "file".to_string(),
            })
            .collect()
    }
}

#[derive(Deserialize)]
pub struct CopyRequest {
    pub path: String,
    #[serde(default)]
    pub items: Vec<FileItem>,
    #[serde(default)]
    pub sources: Vec<String>,
    pub destination: String,
}

impl CopyRequest {
    pub fn resolve_items(&self) -> Vec<FileItem> {
        if !self.items.is_empty() {
            return self.items.clone();
        }
        self.sources
            .iter()
            .map(|s| FileItem {
                path: s.clone(),
                r#type: "file".to_string(),
            })
            .collect()
    }
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

#[derive(Deserialize, Clone)]
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
