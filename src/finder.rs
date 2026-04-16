use actix_multipart::form::MultipartForm;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::io::Cursor;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use zip::{write::FileOptions, ZipWriter};

use crate::payload::{
    ArchiveRequest, CopyRequest, DeleteRequest, MoveRequest, NewFileRequest, NewFolderRequest,
    Query, RenameRequest, SaveRequest, SearchQuery, UnarchiveRequest, UploadForm,
};
use crate::storages::StorageAdapter;
use crate::storages::StorageItem;

// Default configuration functions
#[derive(Clone, Debug, Deserialize)]
pub struct VueFinderConfig {
    pub public_links: Option<std::collections::HashMap<String, String>>,
}

impl VueFinderConfig {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: VueFinderConfig = serde_json::from_str(&content)?;
        Ok(config)
    }
}

impl Default for VueFinderConfig {
    fn default() -> Self {
        Self { public_links: None }
    }
}

#[derive(Debug, Serialize)]
struct FileNode {
    #[serde(flatten)]
    storage_item: StorageItem,
    url: Option<String>,
    // search result supported
    dir: Option<String>,
}

#[derive(Clone)]
pub struct VueFinder {
    pub storages: Arc<std::collections::HashMap<String, Arc<dyn StorageAdapter>>>,
    pub config: Arc<VueFinderConfig>,
}

// Request handling functions
impl VueFinder {
    fn get_default_adapter(&self, adapter: Option<String>) -> String {
        // If adapter is empty, return the first available adapter
        if let Some(adapter) = adapter {
            if self.storages.contains_key(&adapter) {
                return adapter;
            }
        }

        // Return the first available adapter
        self.storages.keys().next().cloned().unwrap_or_default()
    }

    /// Parses a storage name from a path URI like "storage-name://path"
    fn parse_storage_name_from_path(&self, path: &str) -> Option<String> {
        if let Some(pos) = path.find("://") {
            let storage_name = &path[..pos];
            if !storage_name.is_empty() && self.storages.contains_key(storage_name) {
                return Some(storage_name.to_string());
            }
        }
        None
    }

    fn set_public_links(&self, node: &mut FileNode) {
        if let Some(public_links) = &self.config.public_links {
            if node.storage_item.node_type != "dir" {
                for (public_link, domain) in public_links {
                    if node.storage_item.path.starts_with(public_link) {
                        node.url = Some(node.storage_item.path.replace(public_link, domain));
                        break;
                    }
                }
            }
        }
    }

    pub async fn index(data: web::Data<VueFinder>, query: web::Query<Query>) -> HttpResponse {
        let path = &query.path;

        let (storage_name, path_to_list) = if path.is_empty() {
            let default_storage = data.get_default_adapter(None);
            (default_storage, String::new())
        } else {
            match data.parse_storage_name_from_path(path) {
                Some(name) => (name, path.clone()),
                None => {
                    return HttpResponse::BadRequest().json(json!({
                        "status": false,
                        "message": "Invalid path format. Path should include storage prefix (e.g., 'local://')."
                    }))
                }
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        let list_contents = match storage.list_contents(&path_to_list).await {
            Ok(contents) => contents,
            Err(e) => {
                return HttpResponse::InternalServerError().json(json!({
                    "status": false,
                    "message": e.to_string()
                }))
            }
        };

        let files: Vec<FileNode> = list_contents
            .into_iter()
            .map(|item| {
                let mut node = FileNode {
                    storage_item: item,
                    url: None,
                    dir: None,
                };
                data.set_public_links(&mut node);
                node
            })
            .collect();

        let response_path = if path.is_empty() {
            format!("{}://", storage_name)
        } else {
            path.clone()
        };

        HttpResponse::Ok().json(json!({
            "storages": data.storages.keys().collect::<Vec<_>>(),
            "dirname": response_path,
            "files": files,
            "read_only": false
        }))
    }

    pub async fn sub_folders(data: web::Data<VueFinder>, query: web::Query<Query>) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&query.path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        match storage.list_contents(&query.path).await {
            Ok(contents) => {
                let folders: Vec<_> = contents
                    .into_iter()
                    .filter(|item| item.node_type == "dir")
                    .map(|item| {
                        json!({
                            "path": item.path,
                            "basename": item.basename,
                        })
                    })
                    .collect();

                HttpResponse::Ok().json(json!({ "folders": folders }))
            }
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "status": false,
                "message": e.to_string()
            })),
        }
    }

    pub async fn download(data: web::Data<VueFinder>, query: web::Query<Query>) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&query.path) {
            Some(name) => name,
            None => return HttpResponse::BadRequest().finish(),
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => return HttpResponse::BadRequest().finish(),
        };

        match storage.read(&query.path).await {
            Ok(contents) => {
                let filename = Path::new(&query.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();

                let mime = mime_guess::from_path(&query.path).first_or_octet_stream();

                HttpResponse::Ok()
                    .content_type(mime.as_ref())
                    .append_header((
                        "Content-Disposition",
                        format!("attachment; filename=\"{}\"", filename),
                    ))
                    .body(contents)
            }
            Err(_) => HttpResponse::NotFound().finish(),
        }
    }

    pub async fn preview(data: web::Data<VueFinder>, query: web::Query<Query>) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&query.path) {
            Some(name) => name,
            None => return HttpResponse::BadRequest().finish(),
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => return HttpResponse::BadRequest().finish(),
        };

        match storage.read(&query.path).await {
            Ok(contents) => {
                let mime = mime_guess::from_path(&query.path).first_or_octet_stream();

                HttpResponse::Ok()
                    .content_type(mime.as_ref())
                    .body(contents)
            }
            Err(_) => HttpResponse::NotFound().finish(),
        }
    }

    pub async fn search(
        data: web::Data<VueFinder>,
        query: web::Query<SearchQuery>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&query.path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format. Path should include storage prefix (e.g., 'local://')."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        let base_path = query.path.clone();
        let filter = query.filter.clone().unwrap_or_default().to_lowercase();

        async fn search_dir(
            storage: &Arc<dyn StorageAdapter>,
            current_path: String,
            filter: &str,
            results: &mut Vec<FileNode>,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let contents = storage.list_contents(&current_path).await?;

            for item in contents {
                if item.node_type == "file" && item.basename.to_lowercase().contains(filter) {
                    let dir = if let Some(parent) = Path::new(&item.path).parent() {
                        parent.to_string_lossy().to_string()
                    } else {
                        String::new()
                    };

                    results.push(FileNode {
                        storage_item: item,
                        url: None,
                        dir: Some(dir),
                    });
                } else if item.node_type == "dir" {
                    let sub_path = if current_path.is_empty() {
                        item.basename
                    } else {
                        format!("{}/{}", current_path, item.basename)
                    };
                    Box::pin(search_dir(storage, sub_path, filter, results)).await?;
                }
            }
            Ok(())
        }

        let mut files = Vec::new();
        match search_dir(storage, base_path, &filter, &mut files).await {
            Ok(_) => HttpResponse::Ok().json(json!({
                "storages": data.storages.keys().collect::<Vec<_>>(),
                "dirname": query.path,
                "files": files
            })),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "status": false,
                "message": e.to_string()
            })),
        }
    }

    pub async fn new_folder(
        data: web::Data<VueFinder>,
        payload: web::Json<NewFolderRequest>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&payload.path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format. Path should include storage prefix (e.g., 'local://')."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        let new_path = format!("{}/{}", payload.path, payload.name);

        match storage.create_dir(&new_path).await {
            Ok(_) => {
                let query = web::Query(Query {
                    path: payload.path.clone(),
                });
                Self::index(data, query).await
            }

            Err(e) => HttpResponse::InternalServerError().json(json!({
                "status": false,
                "message": e.to_string()
            })),
        }
    }

    pub async fn new_file(
        data: web::Data<VueFinder>,
        payload: web::Json<NewFileRequest>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&payload.path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format. Path should include storage prefix (e.g., 'local://')."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        let new_path = format!("{}/{}", payload.path, payload.name);

        match storage.write(&new_path, vec![]).await {
            Ok(_) => {
                let query = web::Query(Query {
                    path: payload.path.clone(),
                });
                Self::index(data, query).await
            }
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "status": false,
                "message": e.to_string()
            })),
        }
    }

    pub async fn rename(
        data: web::Data<VueFinder>,
        payload: web::Json<RenameRequest>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&payload.path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        let new_path = format!("{}/{}", payload.path, payload.name);

        // Use storage rename which handles both files and directories
        match storage.rename(&payload.item, &new_path).await {
            Ok(_) => {
                let query = web::Query(Query {
                    path: payload.path.clone(),
                });
                Self::index(data, query).await
            }
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "status": false,
                "message": e.to_string()
            })),
        }
    }

    pub async fn r#move(
        data: web::Data<VueFinder>,
        payload: web::Json<MoveRequest>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&payload.destination) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid destination path format."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        // Check if the target path conflicts with existing files
        for source in &payload.sources {
            let target = format!(
                "{}/{}",
                payload.destination,
                Path::new(source)
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap()
            );
            if storage.exists(&target).await.unwrap_or(false) {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "One of the files already exists."
                }));
            }
        }

        // Execute move operation
        for source in &payload.sources {
            let target = format!(
                "{}/{}",
                payload.destination,
                Path::new(source)
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap()
            );

            // Read source file content
            match storage.read(source).await {
                Ok(contents) => {
                    // Write to target location
                    if let Err(e) = storage.write(&target, contents).await {
                        return HttpResponse::InternalServerError().json(json!({
                            "status": false,
                            "message": e.to_string()
                        }));
                    }
                    // Delete source file
                    if let Err(e) = storage.delete(source).await {
                        return HttpResponse::InternalServerError().json(json!({
                            "status": false,
                            "message": e.to_string()
                        }));
                    }
                }
                Err(e) => {
                    return HttpResponse::InternalServerError().json(json!({
                        "status": false,
                        "message": e.to_string()
                    }))
                }
            }
        }

        let query = web::Query(Query {
            path: payload.path.clone(),
        });
        Self::index(data, query).await
    }

    pub async fn copy(
        data: web::Data<VueFinder>,
        payload: web::Json<CopyRequest>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&payload.destination) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid destination path format."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        // Check if the target path conflicts with existing files
        for source in &payload.sources {
            let target = format!(
                "{}/{}",
                payload.destination,
                Path::new(source)
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap()
            );
            if storage.exists(&target).await.unwrap_or(false) {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "One of the files already exists."
                }));
            }
        }

        // Execute copy operation
        for source in &payload.sources {
            let target = format!(
                "{}/{}",
                payload.destination,
                Path::new(source)
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap()
            );

            // Read source file content
            match storage.read(source).await {
                Ok(contents) => {
                    // Write to target location
                    if let Err(e) = storage.write(&target, contents).await {
                        return HttpResponse::InternalServerError().json(json!({
                            "status": false,
                            "message": e.to_string()
                        }));
                    }
                }
                Err(e) => {
                    return HttpResponse::InternalServerError().json(json!({
                        "status": false,
                        "message": e.to_string()
                    }))
                }
            }
        }

        let query = web::Query(Query {
            path: payload.path.clone(),
        });
        Self::index(data, query).await
    }

    pub async fn delete(
        data: web::Data<VueFinder>,
        payload: web::Json<DeleteRequest>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&payload.path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        for item in &payload.items {
            if let Err(e) = storage.delete(&item.path).await {
                return HttpResponse::InternalServerError().json(json!({
                    "status": false,
                    "message": e.to_string()
                }));
            }
        }

        let query = web::Query(Query {
            path: payload.path.clone(),
        });
        Self::index(data, query).await
    }

    pub async fn upload(
        data: web::Data<VueFinder>,
        payload: MultipartForm<UploadForm>,
    ) -> HttpResponse {
        let path = payload.path.to_string();
        let filename = payload.name.to_string();
        
        if path.is_empty() {
            return HttpResponse::BadRequest().json(json!({
                "status": false,
                "message": "Missing path in request body."
            }));
        }

        let storage_name = match data.parse_storage_name_from_path(&path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        // Read file data from TempFile
        let file_path = payload.file.file.path();
        let file_data = match std::fs::read(file_path) {
            Ok(data) => data,
            Err(e) => {
                return HttpResponse::InternalServerError().json(json!({
                    "status": false,
                    "message": format!("Failed to read uploaded file: {}", e)
                }))
            }
        };

        if filename.is_empty() || file_data.is_empty() {
            return HttpResponse::BadRequest().json(json!({
                "status": false,
                "message": "Missing file or filename"
            }));
        }

        // Build file path and save file
        let filepath = format!("{}/{}", path, filename);
        if let Err(e) = storage.write(&filepath, file_data).await {
            return HttpResponse::InternalServerError().json(json!({
                "status": false,
                "message": e.to_string()
            }));
        }

        let query = web::Query(Query {
            path: path.clone(),
        });
        Self::index(data, query).await
    }

    pub async fn archive(
        data: web::Data<VueFinder>,
        payload: web::Json<ArchiveRequest>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&payload.path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        let zip_path = format!("{}/{}.zip", payload.path, payload.name);

        // Check if file already exists
        if storage.exists(&zip_path).await.unwrap_or(false) {
            return HttpResponse::BadRequest().json(json!({
                "status": false,
                "message": "Zip file already exists. Please use a different name."
            }));
        }

        // Create ZIP file
        let mut zip_buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut zip_buffer);
            let mut zip = ZipWriter::new(cursor);

            let options = FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o755);

            for item in &payload.items {
                match storage.read(&item.path).await {
                    Ok(contents) => {
                        let file_name = Path::new(&item.path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or_default();

                        if let Err(e) = zip.start_file(file_name, options) {
                            return HttpResponse::InternalServerError().json(json!({
                                "status": false,
                                "message": format!("Failed to add file to ZIP: {}", e)
                            }));
                        }

                        if let Err(e) = zip.write_all(&contents) {
                            return HttpResponse::InternalServerError().json(json!({
                                "status": false,
                                "message": format!("Failed to write file content: {}", e)
                            }));
                        }
                    }
                    Err(e) => {
                        return HttpResponse::InternalServerError().json(json!({
                            "status": false,
                            "message": format!("Failed to read source file: {}", e)
                        }));
                    }
                }
            }

            if let Err(e) = zip.finish() {
                return HttpResponse::InternalServerError().json(json!({
                    "status": false,
                    "message": format!("Failed to finalize ZIP file: {}", e)
                }));
            }
        }

        // Save ZIP file
        if let Err(e) = storage.write(&zip_path, zip_buffer).await {
            return HttpResponse::InternalServerError().json(json!({
                "status": false,
                "message": format!("Failed to save ZIP file: {}", e)
            }));
        }

        let query = web::Query(Query {
            path: payload.path.clone(),
        });
        Self::index(data, query).await
    }

    pub async fn unarchive(
        data: web::Data<VueFinder>,
        payload: web::Json<UnarchiveRequest>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&payload.path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        // Read ZIP file
        let zip_contents = match storage.read(&payload.item).await {
            Ok(contents) => contents,
            Err(e) => {
                return HttpResponse::InternalServerError().json(json!({
                    "status": false,
                    "message": format!("Failed to read ZIP file: {}", e)
                }));
            }
        };

        let cursor = Cursor::new(zip_contents);
        let mut archive = match zip::ZipArchive::new(cursor) {
            Ok(archive) => archive,
            Err(e) => {
                return HttpResponse::InternalServerError().json(json!({
                    "status": false,
                    "message": format!("Failed to open ZIP file: {}", e)
                }));
            }
        };

        // Extract files
        let extract_path = format!(
            "{}/{}",
            payload.path,
            Path::new(&payload.item)
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
        );

        // Create extraction target directory
        if let Err(e) = storage.create_dir(&extract_path).await {
            return HttpResponse::InternalServerError().json(json!({
                "status": false,
                "message": format!("Failed to create extraction directory: {}", e)
            }));
        }

        for i in 0..archive.len() {
            let mut file = match archive.by_index(i) {
                Ok(file) => file,
                Err(e) => {
                    return HttpResponse::InternalServerError().json(json!({
                        "status": false,
                        "message": format!("Failed to read ZIP file entry: {}", e)
                    }));
                }
            };

            let outpath = format!("{}/{}", extract_path, file.name());

            if file.name().ends_with('/') {
                // Create directory
                if let Err(e) = storage.create_dir(&outpath).await {
                    return HttpResponse::InternalServerError().json(json!({
                        "status": false,
                        "message": format!("Failed to create directory: {}", e)
                    }));
                }
            } else {
                // Ensure parent directory exists
                if let Some(p) = Path::new(&outpath).parent() {
                    if let Some(parent_path) = p.to_str() {
                        if let Err(e) = storage.create_dir(parent_path).await {
                            return HttpResponse::InternalServerError().json(json!({
                                "status": false,
                                "message": format!("Failed to create parent directory: {}", e)
                            }));
                        }
                    }
                }

                // Read and write file contents
                let mut buffer = Vec::new();
                if let Err(e) = std::io::copy(&mut file, &mut buffer) {
                    return HttpResponse::InternalServerError().json(json!({
                        "status": false,
                        "message": format!("Failed to read ZIP file content: {}", e)
                    }));
                }

                if let Err(e) = storage.write(&outpath, buffer).await {
                    return HttpResponse::InternalServerError().json(json!({
                        "status": false,
                        "message": format!("Failed to write extracted file: {}", e)
                    }));
                }
            }
        }

        let query = web::Query(Query {
            path: payload.path.clone(),
        });
        Self::index(data, query).await
    }

    pub async fn save(
        data: web::Data<VueFinder>,
        payload: web::Json<SaveRequest>,
    ) -> HttpResponse {
        let storage_name = match data.parse_storage_name_from_path(&payload.path) {
            Some(name) => name,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid path format."
                }))
            }
        };

        let storage = match data.storages.get(&storage_name) {
            Some(s) => s,
            None => {
                return HttpResponse::BadRequest().json(json!({
                    "status": false,
                    "message": "Invalid storage adapter"
                }))
            }
        };

        match storage
            .write(&payload.path, payload.content.as_bytes().to_vec())
            .await
        {
            Ok(_) => {
                let query = web::Query(Query {
                    path: payload.path.clone(),
                });
                Self::preview(data, query).await
            }
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "status": false,
                "message": e.to_string()
            })),
        }
    }
}
