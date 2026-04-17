use super::{StorageAdapter, StorageError, StorageItem};
use async_trait::async_trait;
use aws_sdk_s3::{
    config::{Credentials, Region},
    error::SdkError,
    operation::{
        get_object::GetObjectError, head_object::HeadObjectError,
        list_objects_v2::ListObjectsV2Error,
    },
    primitives::ByteStream,
    Client,
};
use mime_guess::from_path;
use std::collections::HashMap;
use std::sync::Arc;

const S3_SCHEME: &str = "s3://";

/// Configuration for an S3-compatible storage backend.
/// Works with AWS S3, Cloudflare R2, MinIO, Backblaze B2, etc.
#[derive(Debug, Clone)]
pub struct S3Config {
    /// Bucket name
    pub bucket: String,
    /// AWS region (e.g. "us-east-1"). For R2 use "auto".
    pub region: String,
    /// Access key ID
    pub access_key_id: String,
    /// Secret access key
    pub secret_access_key: String,
    /// Optional custom endpoint URL for S3-compatible services (e.g. Cloudflare R2).
    /// Leave as `None` for standard AWS S3.
    pub endpoint_url: Option<String>,
    /// Optional key prefix — all operations are scoped under this prefix inside the bucket.
    pub prefix: Option<String>,
}

#[derive(Debug)]
pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: String,
    /// The logical name used as the scheme, e.g. "s3" or a custom alias.
    scheme: String,
}

impl S3Storage {
    /// Build an `S3Storage` from an [`S3Config`].
    pub async fn new(cfg: S3Config) -> Self {
        let credentials = Credentials::new(
            &cfg.access_key_id,
            &cfg.secret_access_key,
            None,
            None,
            "vuefinder",
        );

        let mut builder = aws_sdk_s3::Config::builder()
            .region(Region::new(cfg.region))
            .credentials_provider(credentials)
            .force_path_style(cfg.endpoint_url.is_some()); // required for most S3-compatible APIs

        if let Some(endpoint) = cfg.endpoint_url {
            builder = builder.endpoint_url(endpoint);
        }

        let client = Client::from_conf(builder.build());
        let prefix = cfg
            .prefix
            .map(|p| p.trim_matches('/').to_string())
            .unwrap_or_default();

        Self {
            client,
            bucket: cfg.bucket,
            prefix,
            scheme: S3_SCHEME.trim_end_matches("://").to_string(),
        }
    }

    /// Convenience constructor that registers the adapter in a `HashMap` like `LocalStorage::setup`.
    pub async fn setup(cfg: S3Config) -> Arc<HashMap<String, Arc<dyn StorageAdapter>>> {
        let mut storages = HashMap::new();
        let storage = Arc::new(Self::new(cfg).await) as Arc<dyn StorageAdapter>;
        storages.insert(storage.name(), storage);
        Arc::new(storages)
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Strip the scheme prefix and return the bare logical path.
    fn strip_scheme<'a>(&self, path: &'a str) -> &'a str {
        let scheme = format!("{}://", self.scheme);
        path.trim_start_matches(scheme.as_str())
            .trim_start_matches('/')
    }

    /// Convert a logical path (relative to the adapter root) into an S3 object key.
    fn to_key(&self, path: &str) -> String {
        let bare = self.strip_scheme(path);
        if self.prefix.is_empty() {
            bare.to_string()
        } else {
            format!("{}/{}", self.prefix, bare)
        }
    }

    /// Convert an S3 object key back to a logical path (scheme-prefixed).
    fn key_to_path(&self, key: &str) -> String {
        let relative = if self.prefix.is_empty() {
            key.to_string()
        } else {
            key.trim_start_matches(&format!("{}/", self.prefix))
                .to_string()
        };
        format!("{}://{}", self.scheme, relative)
    }

    /// The S3 "directory" key suffix used to represent a folder (zero-byte object ending with `/`).
    fn dir_key(&self, path: &str) -> String {
        let key = self.to_key(path);
        if key.ends_with('/') {
            key
        } else {
            format!("{}/", key)
        }
    }
}

#[async_trait]
impl StorageAdapter for S3Storage {
    fn name(&self) -> String {
        self.scheme.clone()
    }

    /// List the immediate children of a "directory" prefix in S3.
    /// Uses the delimiter `/` so that sub-prefixes are returned as common prefixes (dirs).
    async fn list_contents(
        &self,
        path: &str,
    ) -> Result<Vec<StorageItem>, Box<dyn std::error::Error>> {
        let mut prefix = self.to_key(path);
        if !prefix.is_empty() && !prefix.ends_with('/') {
            prefix.push('/');
        }

        let mut items = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .delimiter("/")
                .set_prefix(if prefix.is_empty() {
                    None
                } else {
                    Some(prefix.clone())
                });

            if let Some(token) = continuation_token.take() {
                req = req.continuation_token(token);
            }

            let resp = req.send().await.map_err(|e| {
                Box::new(map_list_error(e)) as Box<dyn std::error::Error>
            })?;

            // Common prefixes → directories
            for cp in resp.common_prefixes() {
                let key = cp.prefix().unwrap_or_default();
                // strip trailing slash for display
                let display_key = key.trim_end_matches('/');
                let logical_path = self.key_to_path(display_key);
                let basename = display_key
                    .rsplit('/')
                    .next()
                    .unwrap_or(display_key)
                    .to_string();

                items.push(StorageItem {
                    node_type: "dir".to_string(),
                    path: logical_path,
                    basename,
                    extension: None,
                    mime_type: None,
                    last_modified: None,
                    size: None,
                });
            }

            // Objects → files (skip the prefix "directory" placeholder itself)
            for obj in resp.contents() {
                let key = obj.key().unwrap_or_default();
                if key.ends_with('/') {
                    continue; // skip directory markers
                }

                let logical_path = self.key_to_path(key);
                let basename = key.rsplit('/').next().unwrap_or(key).to_string();
                let extension = std::path::Path::new(&basename)
                    .extension()
                    .map(|e| e.to_string_lossy().into_owned());
                let mime_type = Some(
                    from_path(&basename)
                        .first_or_octet_stream()
                        .essence_str()
                        .to_owned(),
                );
                let last_modified = obj
                    .last_modified()
                    .and_then(|dt| u64::try_from(dt.secs()).ok());
                let size = obj.size().and_then(|s| u64::try_from(s).ok());

                items.push(StorageItem {
                    node_type: "file".to_string(),
                    path: logical_path,
                    basename,
                    extension,
                    mime_type,
                    last_modified,
                    size,
                });
            }

            if resp.is_truncated().unwrap_or(false) {
                continuation_token = resp.next_continuation_token().map(str::to_string);
            } else {
                break;
            }
        }

        Ok(items)
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        let key = self.to_key(path);
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| match e.into_service_error() {
                GetObjectError::NoSuchKey(_) => StorageError::NotFound(path.to_string()),
                other => StorageError::InvalidPath(format!("S3 get error: {}", other)),
            })?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| StorageError::InvalidPath(format!("S3 stream error: {}", e)))?
            .into_bytes();

        Ok(bytes.to_vec())
    }

    async fn write(&self, path: &str, contents: Vec<u8>) -> Result<(), StorageError> {
        let key = self.to_key(path);
        let body = ByteStream::from(contents);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(body)
            .send()
            .await
            .map_err(|e| StorageError::InvalidPath(format!("S3 put error for '{}': {}", path, e)))?;

        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        // Check if it's a directory (prefix) first
        let dir_key = self.dir_key(path);
        let is_dir = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&dir_key)
            .max_keys(1)
            .send()
            .await
            .map(|r| r.key_count().unwrap_or(0) > 0)
            .unwrap_or(false);

        if is_dir {
            // Delete all objects under the prefix
            let mut continuation_token: Option<String> = None;
            loop {
                let mut req = self
                    .client
                    .list_objects_v2()
                    .bucket(&self.bucket)
                    .prefix(&dir_key);

                if let Some(token) = continuation_token.take() {
                    req = req.continuation_token(token);
                }

                let resp = req.send().await.map_err(|e| {
                    StorageError::InvalidPath(format!("S3 list error: {}", e))
                })?;

                for obj in resp.contents() {
                    let key = obj.key().unwrap_or_default();
                    self.client
                        .delete_object()
                        .bucket(&self.bucket)
                        .key(key)
                        .send()
                        .await
                        .map_err(|e| {
                            StorageError::InvalidPath(format!("S3 delete error: {}", e))
                        })?;
                }

                if resp.is_truncated().unwrap_or(false) {
                    continuation_token = resp.next_continuation_token().map(str::to_string);
                } else {
                    break;
                }
            }
            return Ok(());
        }

        // Single object delete
        let key = self.to_key(path);
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::InvalidPath(format!("S3 delete error for '{}': {}", path, e)))?;

        Ok(())
    }

    /// In S3 there are no real directories; we create a zero-byte placeholder object ending with `/`.
    async fn create_dir(&self, path: &str) -> Result<(), StorageError> {
        let key = self.dir_key(path);
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(vec![]))
            .send()
            .await
            .map_err(|e| {
                StorageError::InvalidPath(format!("S3 create_dir error for '{}': {}", path, e))
            })?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, StorageError> {
        let key = self.to_key(path);

        // Check as a file first
        let file_result = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;

        match file_result {
            Ok(_) => return Ok(true),
            Err(e) => match e.into_service_error() {
                HeadObjectError::NotFound(_) => {}
                other => {
                    return Err(StorageError::InvalidPath(format!(
                        "S3 head error: {}",
                        other
                    )))
                }
            },
        }

        // Check as a directory prefix
        let dir_key = self.dir_key(path);
        let resp = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&dir_key)
            .max_keys(1)
            .send()
            .await
            .map_err(|e| StorageError::InvalidPath(format!("S3 list error: {}", e)))?;

        Ok(resp.key_count().unwrap_or(0) > 0)
    }

    /// S3 has no native rename; we copy then delete.
    async fn rename(&self, old_path: &str, new_path: &str) -> Result<(), StorageError> {
        let old_key = self.to_key(old_path);
        let new_key = self.to_key(new_path);

        // Check if it's a directory
        let dir_old = self.dir_key(old_path);
        let is_dir = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&dir_old)
            .max_keys(1)
            .send()
            .await
            .map(|r| r.key_count().unwrap_or(0) > 0)
            .unwrap_or(false);

        if is_dir {
            // List all objects under old prefix and copy each one
            let mut continuation_token: Option<String> = None;
            loop {
                let mut req = self
                    .client
                    .list_objects_v2()
                    .bucket(&self.bucket)
                    .prefix(&dir_old);

                if let Some(token) = continuation_token.take() {
                    req = req.continuation_token(token);
                }

                let resp = req.send().await.map_err(|e| {
                    StorageError::InvalidPath(format!("S3 list error: {}", e))
                })?;

                let dir_new = self.dir_key(new_path);
                for obj in resp.contents() {
                    let src_key = obj.key().unwrap_or_default();
                    let suffix = src_key.trim_start_matches(&dir_old);
                    let dst_key = format!("{}{}", dir_new, suffix);

                    self.copy_object(src_key, &dst_key).await?;
                    self.client
                        .delete_object()
                        .bucket(&self.bucket)
                        .key(src_key)
                        .send()
                        .await
                        .map_err(|e| {
                            StorageError::InvalidPath(format!("S3 delete error: {}", e))
                        })?;
                }

                if resp.is_truncated().unwrap_or(false) {
                    continuation_token = resp.next_continuation_token().map(str::to_string);
                } else {
                    break;
                }
            }
            return Ok(());
        }

        // Single file rename
        self.copy_object(&old_key, &new_key).await?;
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&old_key)
            .send()
            .await
            .map_err(|e| StorageError::InvalidPath(format!("S3 delete error: {}", e)))?;

        Ok(())
    }
}

impl S3Storage {
    async fn copy_object(&self, src_key: &str, dst_key: &str) -> Result<(), StorageError> {
        let copy_source = format!("{}/{}", self.bucket, src_key);
        self.client
            .copy_object()
            .bucket(&self.bucket)
            .copy_source(&copy_source)
            .key(dst_key)
            .send()
            .await
            .map_err(|e| StorageError::InvalidPath(format!("S3 copy error: {}", e)))?;
        Ok(())
    }
}

// ── free helpers (can't be methods because they're used in closures) ──────────

fn map_list_error(e: SdkError<ListObjectsV2Error>) -> StorageError {
    StorageError::InvalidPath(format!("S3 list error: {}", e))
}
