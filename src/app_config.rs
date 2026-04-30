use actix_web::dev::ServiceRequest;
use actix_web::{dev::ServiceFactory, web, App, Error};
use std::collections::HashMap;
use std::sync::Arc;

use crate::finder::{VueFinder, VueFinderConfig};
use crate::router::{
    archive_handler, copy_handler, delete_handler, download_handler, index_handler,
    move_handler, new_file_handler, new_folder_handler, preview_handler, rename_handler,
    save_handler, search_handler, unarchive_handler, upload_handler,
};
use crate::storages::StorageAdapter;
use crate::thumbnail::ThumbnailCache;

#[derive(Clone)]
pub struct VueFinderAppConfig {
    pub api_path: String,
    pub json_limit: usize,
    pub payload_limit: usize,
    pub storages: Arc<HashMap<String, Arc<dyn StorageAdapter>>>,
    pub finder_config: Arc<VueFinderConfig>,
}

impl Default for VueFinderAppConfig {
    fn default() -> Self {
        Self {
            api_path: "/api".to_string(),
            json_limit: 100 * 1024 * 1024,    // 100MB
            payload_limit: 100 * 1024 * 1024, // 100MB
            storages: Arc::new(HashMap::new()),
            finder_config: Arc::new(VueFinderConfig::default()),
        }
    }
}

pub trait VueFinderAppExt {
    fn configure_vuefinder(self, config: VueFinderAppConfig) -> Self;
}

impl<T> VueFinderAppExt for App<T>
where
    T: ServiceFactory<ServiceRequest, Config = (), Error = Error, InitError = ()>,
{
    fn configure_vuefinder(self, config: VueFinderAppConfig) -> Self {
        let vue_finder = web::Data::new(VueFinder {
            storages: config.storages,
            config: config.finder_config,
            thumbnail_cache: ThumbnailCache::new(),
        });

        self.app_data(web::JsonConfig::default().limit(config.json_limit))
            .app_data(web::PayloadConfig::default().limit(config.payload_limit))
            .app_data(vue_finder)
            .service(
                web::scope(&config.api_path)
                    .service(index_handler)
                    .service(search_handler)
                    .service(preview_handler)
                    .service(download_handler)
                    .service(new_folder_handler)
                    .service(new_file_handler)
                    .service(rename_handler)
                    .service(move_handler)
                    .service(copy_handler)
                    .service(delete_handler)
                    .service(upload_handler)
                    .service(archive_handler)
                    .service(unarchive_handler)
                    .service(save_handler),
            )
    }
}
