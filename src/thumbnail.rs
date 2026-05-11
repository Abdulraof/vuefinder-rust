use image::{GenericImageView, ImageFormat};
use lru::LruCache;
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const THUMBNAIL_SIZE: u32 = 200;
const CACHE_SIZE: usize = 1000;
const JPEG_QUALITY: u8 = 80;

#[derive(Clone)]
pub struct ThumbnailCache {
    pub(crate) cache: Arc<Mutex<LruCache<String, CachedThumbnail>>>,
}

#[derive(Clone)]
pub(crate) struct CachedThumbnail {
    pub(crate) data: Vec<u8>,
    pub(crate) mime_type: String,
    #[allow(dead_code)]
    pub(crate) created_at: u64,
}

impl ThumbnailCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(CACHE_SIZE).unwrap(),
            ))),
        }
    }

    /// Check if a file is an image based on its MIME type
    pub fn is_image(mime_type: &str) -> bool {
        matches!(
            mime_type,
            "image/jpeg"
                | "image/jpg"
                | "image/png"
                | "image/gif"
                | "image/webp"
                | "image/bmp"
                | "image/tiff"
                | "image/svg+xml"
        )
    }

    /// Generate a cache key for a file
    fn generate_cache_key(path: &str, last_modified: Option<u64>) -> String {
        format!("{}:{}", path, last_modified.unwrap_or(0))
    }

    /// Get thumbnail from cache or generate it
    pub async fn get_thumbnail(
        &self,
        path: &str,
        file_data: &[u8],
        mime_type: &str,
        last_modified: Option<u64>,
    ) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
        let cache_key = Self::generate_cache_key(path, last_modified);

        // Check cache first with a single lock
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(&cache_key) {
                return Ok((cached.data.clone(), cached.mime_type.clone()));
            }
        }

        // Generate thumbnail if not in cache (outside the lock)
        let (thumbnail_data, thumbnail_mime) =
            self.generate_thumbnail(file_data, mime_type).await?;

        // Store in cache with a single lock
        {
            let mut cache = self.cache.lock().unwrap();
            let cached_thumbnail = CachedThumbnail {
                data: thumbnail_data.clone(),
                mime_type: thumbnail_mime.clone(),
                created_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };
            cache.put(cache_key, cached_thumbnail);
        }

        Ok((thumbnail_data, thumbnail_mime))
    }

    /// Generate a thumbnail from image data
    async fn generate_thumbnail(
        &self,
        file_data: &[u8],
        mime_type: &str,
    ) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
        // Handle SVG separately as it's not supported by the image crate
        if mime_type == "image/svg+xml" {
            // For SVG, we'll return the original data as thumbnail
            // In a production environment, you might want to use a library like resvg
            return Ok((file_data.to_vec(), mime_type.to_string()));
        }

        // Parse the image format
        let format = match mime_type {
            "image/jpeg" | "image/jpg" => ImageFormat::Jpeg,
            "image/png" => ImageFormat::Png,
            "image/gif" => ImageFormat::Gif,
            "image/webp" => ImageFormat::WebP,
            "image/bmp" => ImageFormat::Bmp,
            "image/tiff" => ImageFormat::Tiff,
            _ => return Err("Unsupported image format".into()),
        };

        // For very small files, might already be a thumbnail
        if file_data.len() < 50_000 {
            // Try to decode and check dimensions
            if let Ok(img) = image::load_from_memory_with_format(file_data, format) {
                let (width, height) = img.dimensions();
                if width <= THUMBNAIL_SIZE && height <= THUMBNAIL_SIZE {
                    // Already small enough, return as-is if JPEG
                    if matches!(format, ImageFormat::Jpeg) {
                        return Ok((file_data.to_vec(), "image/jpeg".to_string()));
                    }
                }
            }
        }

        // Clone data for blocking operation
        let file_data = file_data.to_vec();
        
        // Move CPU-intensive work to blocking thread pool
        let thumbnail_data = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            // Load the image
            let img = image::load_from_memory_with_format(&file_data, format)
                .map_err(|e| format!("Failed to load image: {}", e))?;
            
            let (width, height) = img.dimensions();
            
            // If image is already small, just re-encode with lower quality
            if width <= THUMBNAIL_SIZE && height <= THUMBNAIL_SIZE {
                let mut buffer = Vec::new();
                let mut cursor = Cursor::new(&mut buffer);
                let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, JPEG_QUALITY);
                img.write_with_encoder(encoder)
                    .map_err(|e| format!("Failed to encode image: {}", e))?;
                return Ok(buffer);
            }

            // Calculate new dimensions
            let ratio = (THUMBNAIL_SIZE as f32) / (width.max(height) as f32);
            let new_width = (width as f32 * ratio) as u32;
            let new_height = (height as f32 * ratio) as u32;

            // For very large images (>2000px), use two-pass resize for speed
            let thumbnail = if width > 2000 || height > 2000 {
                // First pass: quick resize to intermediate size using Nearest (fastest)
                let intermediate_size = THUMBNAIL_SIZE * 4;
                let intermediate_ratio = (intermediate_size as f32) / (width.max(height) as f32);
                let intermediate_width = (width as f32 * intermediate_ratio) as u32;
                let intermediate_height = (height as f32 * intermediate_ratio) as u32;
                
                let intermediate = img.resize_exact(
                    intermediate_width,
                    intermediate_height,
                    image::imageops::FilterType::Nearest
                );
                
                // Second pass: resize to final size with Triangle for quality
                intermediate.resize_exact(
                    new_width,
                    new_height,
                    image::imageops::FilterType::Triangle
                )
            } else {
                // Single pass for smaller images
                img.resize_exact(
                    new_width, 
                    new_height, 
                    image::imageops::FilterType::Triangle
                )
            };

            // Encode as JPEG with controlled quality for speed
            let mut buffer = Vec::new();
            let mut cursor = Cursor::new(&mut buffer);
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, JPEG_QUALITY);
            thumbnail.write_with_encoder(encoder)
                .map_err(|e| format!("Failed to encode thumbnail: {}", e))?;

            Ok(buffer)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))??;

        Ok((thumbnail_data, "image/jpeg".to_string()))
    }



    /// Clear expired entries from cache (optional maintenance function)
    pub fn cleanup_cache(&self, _max_age_seconds: u64) {
        let _cache = self.cache.lock().unwrap();
        let _current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Note: LruCache doesn't have a way to iterate and remove based on condition
        // In a production environment, you might want to use a different cache implementation
        // or implement a custom cleanup mechanism
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.lock().unwrap();
        (cache.len(), cache.cap().get())
    }
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self::new()
    }
}
