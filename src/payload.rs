use serde::Deserialize;

#[derive(Deserialize)]
pub struct Query {
    pub q: String,
    pub adapter: Option<String>,
    pub path: Option<String>,
    pub filter: Option<String>,
}

#[derive(Deserialize)]
pub struct NewFolderRequest {
    pub name: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct NewFileRequest {
    pub name: String,
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub name: String,
    pub item: String,
}

#[derive(Deserialize)]
pub struct MoveRequest {
    pub item: String,
    pub items: Vec<FileItem>,
}

#[derive(Deserialize)]
pub struct DeleteRequest {
    pub items: Vec<FileItem>,
}

#[derive(Deserialize)]
pub struct ArchiveRequest {
    pub name: String,
    pub items: Vec<FileItem>,
}

#[derive(Deserialize)]
pub struct UnarchiveRequest {
    pub item: String,
}

#[derive(Deserialize)]
pub struct SaveRequest {
    pub content: String,
}

#[derive(Deserialize)]
pub struct FileItem {
    pub path: String,
}
