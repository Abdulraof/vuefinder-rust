use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Path not found: {0}")]
    NotFound(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

#[async_trait]
pub trait StorageAdapter: Send + Sync {
    fn name(&self) -> String;
    async fn list_contents(
        &self,
        path: &str,
    ) -> Result<Vec<StorageItem>, Box<dyn std::error::Error>>;
    async fn read(&self, path: &str) -> Result<Vec<u8>, StorageError>;
    async fn write(&self, path: &str, contents: Vec<u8>) -> Result<(), StorageError>;
    async fn delete(&self, path: &str) -> Result<(), StorageError>;
    async fn create_dir(&self, path: &str) -> Result<(), StorageError>;
    async fn exists(&self, path: &str) -> Result<bool, StorageError>;
    async fn rename(&self, old_path: &str, new_path: &str) -> Result<(), StorageError>;
}

#[derive(Debug, Serialize)]
pub struct StorageItem {
    #[serde(rename = "type")]
    pub node_type: String,
    pub path: String,
    pub basename: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub last_modified: Option<u64>,
    #[serde(rename = "file_size")]
    pub size: Option<u64>,
}

pub mod local;

#[cfg(feature = "s3")]
pub mod s3;
