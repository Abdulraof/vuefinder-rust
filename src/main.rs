use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::{App, HttpServer};
use clap::Parser;
use env_logger::Env;
use std::sync::Arc;

use std::collections::HashMap;
use vuefinder::{
    app_config::{VueFinderAppConfig, VueFinderAppExt},
    finder::VueFinderConfig,
    storages::{local::LocalStorage, StorageAdapter},
    S3Config, S3Storage,
};

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// Server listening port
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Server binding address
    #[arg(short = 'b', long, default_value = "127.0.0.1")]
    host: String,

    /// Local storage path
    #[arg(short = 'l', long, default_value = "./storage")]
    local_storage: String,

    /// Finder config file path
    #[arg(short, long, default_value = "./vuefinder.json")]
    config: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let args = Args::parse();

    env_logger::init_from_env(Env::default().default_filter_or("info"));

    // Ensure storage directory exists
    tokio::fs::create_dir_all(&args.local_storage).await?;

    let config = VueFinderConfig::from_file(&args.config).unwrap_or_default();

    let mut storages: HashMap<String, Arc<dyn StorageAdapter>> = HashMap::new();

    let local = Arc::new(LocalStorage::new(&args.local_storage)) as Arc<dyn StorageAdapter>;
    storages.insert(local.name(), local);

    let s3 = Arc::new(S3Storage::new(S3Config {
        bucket: std::env::var("S3_BUCKET").unwrap_or_default(),
        region: std::env::var("S3_REGION").unwrap_or_else(|_| "auto".into()),
        access_key_id: std::env::var("S3_ACCESS_KEY_ID").unwrap_or_default(),
        secret_access_key: std::env::var("S3_SECRET_ACCESS_KEY").unwrap_or_default(),
        endpoint_url: std::env::var("S3_ENDPOINT_URL").ok(),
        prefix: std::env::var("S3_PREFIX").ok(),
    }).await) as Arc<dyn StorageAdapter>;
    storages.insert(s3.name(), s3);

    let app_config = VueFinderAppConfig {
        storages: Arc::new(storages),
        finder_config: Arc::new(config),
        ..VueFinderAppConfig::default()
    };

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(Logger::default())
            .wrap(cors)
            .configure_vuefinder(app_config.clone())
    })
    .bind(format!("{}:{}", args.host, args.port))?
    .run()
    .await
}
