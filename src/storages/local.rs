use super::{StorageAdapter, StorageError, StorageItem};
use async_trait::async_trait;
use mime_guess::from_path;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;

const LOCAL_SCHEME: &str = "local://";

#[derive(Debug)]
pub struct LocalStorage {
    root: String,
}

impl LocalStorage {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
        }
    }

    pub fn setup(path: &str) -> Arc<HashMap<String, Arc<dyn StorageAdapter>>> {
        let mut storages = HashMap::new();
        let storage = Arc::new(Self::new(path)) as Arc<dyn StorageAdapter>;
        storages.insert(storage.name(), storage);
        Arc::new(storages)
    }

    // Parse and validate path
    fn resolve_path(&self, path: &str) -> Result<PathBuf, StorageError> {
        let clean_path = path
            .trim_start_matches(LOCAL_SCHEME)
            .trim_start_matches('/');

        // Convert to absolute path and normalize
        let full_path = PathBuf::from(&self.root)
            .canonicalize()
            .map_err(StorageError::Io)?
            .join(clean_path);

        // Try to canonicalize the full path if it exists
        let canonical_path = if full_path.exists() {
            full_path.canonicalize().map_err(StorageError::Io)?
        } else {
            // For non-existent paths, walk up to find the nearest existing ancestor,
            // canonicalize it, and append the remaining components.
            let mut ancestor = full_path.as_path();
            let mut remaining = Vec::new();
            loop {
                if ancestor.exists() {
                    break;
                }
                remaining.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| {
                            StorageError::InvalidPath(
                                "Invalid path: no parent directory".to_string(),
                            )
                        })?
                        .to_owned(),
                );
                ancestor = ancestor.parent().ok_or_else(|| {
                    StorageError::InvalidPath("Invalid path: no parent directory".to_string())
                })?;
            }
            let mut result = ancestor.canonicalize().map_err(StorageError::Io)?;
            for component in remaining.into_iter().rev() {
                result = result.join(component);
            }
            result
        };

        // Get canonical root path
        let root_path = PathBuf::from(&self.root)
            .canonicalize()
            .map_err(StorageError::Io)?;

        // Security check: ensure path is under root directory
        if !canonical_path.starts_with(&root_path) {
            return Err(StorageError::InvalidPath(
                "Path attempts to escape root directory".to_string(),
            ));
        }

        Ok(canonical_path)
    }
}

#[async_trait]
impl StorageAdapter for LocalStorage {
    fn name(&self) -> String {
        LOCAL_SCHEME.trim_end_matches("://").to_string()
    }

    async fn list_contents(
        &self,
        path: &str,
    ) -> Result<Vec<StorageItem>, Box<dyn std::error::Error>> {
        let full_path = self.resolve_path(path)?;
        let mut entries = Vec::new();

        let mut read_dir = fs::read_dir(&full_path).await?;

        // Get canonical root path
        let root_path = PathBuf::from(&self.root)
            .canonicalize()
            .map_err(StorageError::Io)?;

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            let path_buf = entry.path();

            // Calculate relative path from root
            let relative_path = path_buf
                .strip_prefix(&root_path)
                .unwrap_or(&path_buf)
                .to_string_lossy()
                .into_owned();

            let basename = path_buf
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            let extension = path_buf
                .extension()
                .map(|ext| ext.to_string_lossy().into_owned());

            let mime_type = if metadata.is_file() {
                Some(
                    from_path(&path_buf)
                        .first_or_octet_stream()
                        .essence_str()
                        .to_owned(),
                )
            } else {
                None
            };

            let last_modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());

            let size = if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            };

            entries.push(StorageItem {
                node_type: if metadata.is_dir() {
                    "dir".to_string()
                } else {
                    "file".to_string()
                },
                path: format!("{}{}", LOCAL_SCHEME, relative_path),
                basename,
                extension,
                mime_type,
                last_modified,
                size,
            });
        }

        Ok(entries)
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        let full_path = self.resolve_path(path)?;

        match fs::read(&full_path).await {
            Ok(contents) => Ok(contents),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                Err(StorageError::NotFound(path.to_string()))
            }
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn write(&self, path: &str, contents: Vec<u8>) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path)?;

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(&full_path, contents).await?;
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path)?;

        match fs::metadata(&full_path).await {
            Ok(metadata) => {
                if metadata.is_dir() {
                    fs::remove_dir_all(&full_path).await?;
                } else {
                    fs::remove_file(&full_path).await?;
                }
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                Err(StorageError::NotFound(path.to_string()))
            }
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn create_dir(&self, path: &str) -> Result<(), StorageError> {
        let full_path = self.resolve_path(path)?;

        match fs::create_dir_all(&full_path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn exists(&self, path: &str) -> Result<bool, StorageError> {
        let full_path = self.resolve_path(path)?;
        Ok(fs::try_exists(&full_path).await?)
    }

    async fn rename(&self, old_path: &str, new_path: &str) -> Result<(), StorageError> {
        let old_full_path = self.resolve_path(old_path)?;
        let new_full_path = self.resolve_path(new_path)?;

        // Ensure parent directory of new path exists
        if let Some(parent) = new_full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        match fs::rename(&old_full_path, &new_full_path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                Err(StorageError::NotFound(old_path.to_string()))
            }
            Err(e) => Err(StorageError::Io(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_str().unwrap());

        // Test create directory
        storage.create_dir("test_dir").await.unwrap();
        assert!(storage.exists("test_dir").await.unwrap());

        // Test write file
        storage
            .write("test_dir/test.txt", b"Hello".to_vec())
            .await
            .unwrap();
        assert!(storage.exists("test_dir/test.txt").await.unwrap());

        // Test read file
        let contents = storage.read("test_dir/test.txt").await.unwrap();
        assert_eq!(contents, b"Hello");

        // Test list contents
        let entries = storage.list_contents("test_dir").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "local://test_dir/test.txt");
        assert_eq!(entries[0].node_type, "file");
        assert_eq!(entries[0].extension.as_deref(), Some("txt"));
        assert_eq!(entries[0].mime_type.as_deref(), Some("text/plain"));

        // Test delete
        storage.delete("test_dir/test.txt").await.unwrap();
        assert!(!storage.exists("test_dir/test.txt").await.unwrap());

        // Test delete directory
        storage.delete("test_dir").await.unwrap();
        assert!(!storage.exists("test_dir").await.unwrap());
    }

    #[tokio::test]
    async fn test_invalid_paths() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_str().unwrap());

        // Test path traversal attack
        assert!(storage.read("../outside.txt").await.is_err());
        assert!(storage.read("test/../../outside.txt").await.is_err());
        assert!(storage.write("../outside.txt", vec![]).await.is_err());
        assert!(storage
            .write("test/../../outside.txt", vec![])
            .await
            .is_err());

        // Test absolute path
        assert!(storage.read("/etc/passwd").await.is_err());

        // Test non-existent file
        assert!(matches!(
            storage.read("nonexistent.txt").await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_rename_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_str().unwrap());

        // Create a file
        storage.write("test.txt", b"Hello".to_vec()).await.unwrap();
        assert!(storage.exists("test.txt").await.unwrap());

        // Rename file
        storage.rename("test.txt", "renamed.txt").await.unwrap();
        assert!(!storage.exists("test.txt").await.unwrap());
        assert!(storage.exists("renamed.txt").await.unwrap());

        // Verify content
        let contents = storage.read("renamed.txt").await.unwrap();
        assert_eq!(contents, b"Hello");
    }

    #[tokio::test]
    async fn test_rename_directory() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_str().unwrap());

        // Create a directory with a file
        storage.create_dir("test_dir").await.unwrap();
        storage
            .write("test_dir/file.txt", b"World".to_vec())
            .await
            .unwrap();
        assert!(storage.exists("test_dir").await.unwrap());

        // Rename directory
        storage.rename("test_dir", "renamed_dir").await.unwrap();
        assert!(!storage.exists("test_dir").await.unwrap());
        assert!(storage.exists("renamed_dir").await.unwrap());
        assert!(storage.exists("renamed_dir/file.txt").await.unwrap());

        // Verify file content still accessible
        let contents = storage.read("renamed_dir/file.txt").await.unwrap();
        assert_eq!(contents, b"World");
    }
}
