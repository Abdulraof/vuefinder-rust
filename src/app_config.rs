use actix_web::dev::ServiceRequest;
use actix_web::{dev::ServiceFactory, web, App, Error};
use std::collections::HashMap;
use std::sync::Arc;

use crate::finder::{VueFinder, VueFinderConfig};
use crate::router::{finder_router, new_folder_handler};
use crate::storages::StorageAdapter;

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
        });

        self.app_data(web::JsonConfig::default().limit(config.json_limit))
            .app_data(web::PayloadConfig::default().limit(config.payload_limit))
            .app_data(vue_finder)
            .service(
                web::scope(&config.api_path)
                    .service(new_folder_handler)
                    .service(web::resource("").route(web::route().to(finder_router))),
            )
    }
}
