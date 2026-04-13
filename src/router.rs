use actix_multipart::form::MultipartForm;
use actix_web::{get, post, web, HttpResponse};

use crate::payload::{
    ArchiveRequest, CopyRequest, DeleteRequest, MoveRequest, NewFileRequest, NewFolderRequest,
    Query, RenameRequest, SaveRequest, SearchQuery, UnarchiveRequest, UploadForm,
};

use crate::finder::VueFinder;

#[get("/")]
pub async fn index_handler(data: web::Data<VueFinder>, query: web::Query<Query>) -> HttpResponse {
    VueFinder::index(data, query).await
}

#[get("/search")]
pub async fn search_handler(
    data: web::Data<VueFinder>,
    query: web::Query<SearchQuery>,
) -> HttpResponse {
    VueFinder::search(data, query).await
}

#[get("/preview")]
pub async fn preview_handler(data: web::Data<VueFinder>, query: web::Query<Query>) -> HttpResponse {
    VueFinder::preview(data, query).await
}

#[get("/download")]
pub async fn download_handler(
    data: web::Data<VueFinder>,
    query: web::Query<Query>,
) -> HttpResponse {
    VueFinder::download(data, query).await
}

#[post("/create-folder")]
pub async fn new_folder_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<NewFolderRequest>,
) -> HttpResponse {
    VueFinder::new_folder(data, payload).await
}

#[post("/create-file")]
pub async fn new_file_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<NewFileRequest>,
) -> HttpResponse {
    VueFinder::new_file(data, payload).await
}

#[post("/rename")]
pub async fn rename_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<RenameRequest>,
) -> HttpResponse {
    VueFinder::rename(data, payload).await
}

#[post("/move")]
pub async fn move_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<MoveRequest>,
) -> HttpResponse {
    VueFinder::r#move(data, payload).await
}

#[post("/copy")]
pub async fn copy_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<CopyRequest>,
) -> HttpResponse {
    VueFinder::copy(data, payload).await
}

#[post("/delete")]
pub async fn delete_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<DeleteRequest>,
) -> HttpResponse {
    VueFinder::delete(data, payload).await
}

#[post("/upload")]
pub async fn upload_handler(
    data: web::Data<VueFinder>,
    payload: MultipartForm<UploadForm>,
) -> HttpResponse {
    VueFinder::upload(data, payload).await
}

#[post("/archive")]
pub async fn archive_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<ArchiveRequest>,
) -> HttpResponse {
    VueFinder::archive(data, payload).await
}

#[post("/unarchive")]
pub async fn unarchive_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<UnarchiveRequest>,
) -> HttpResponse {
    VueFinder::unarchive(data, payload).await
}

#[post("/save")]
pub async fn save_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<SaveRequest>,
) -> HttpResponse {
    VueFinder::save(data, payload).await
}
