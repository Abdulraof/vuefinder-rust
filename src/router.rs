use actix_multipart::Multipart;
use actix_web::{post, web, HttpRequest, HttpResponse};

use crate::payload::{
    ArchiveRequest, DeleteRequest, MoveRequest, NewFileRequest, NewFolderRequest, Query,
    RenameRequest, SaveRequest, UnarchiveRequest,
};

use crate::finder::VueFinder;

#[post("/create-folder")]
pub async fn new_folder_handler(
    data: web::Data<VueFinder>,
    payload: web::Json<NewFolderRequest>,
) -> HttpResponse {
    println!("Received new folder request: {:?}", payload.path);
    VueFinder::new_folder(data, payload).await
}

pub async fn finder_router(
    req: HttpRequest,
    data: web::Data<VueFinder>,
    query: web::Query<Query>,
    payload: Option<web::Either<web::Json<serde_json::Value>, Multipart>>,
) -> Result<HttpResponse, actix_web::Error> {
    match *req.method() {
        actix_web::http::Method::GET => match query.q.as_str() {
            "index" => Ok(VueFinder::index(data, query).await),
            "subfolders" => Ok(VueFinder::sub_folders(data, query).await),
            "download" => Ok(VueFinder::download(data, query).await),
            "preview" => Ok(VueFinder::preview(data, query).await),
            "search" => Ok(VueFinder::search(data, query).await),
            _ => Ok(HttpResponse::BadRequest().finish()),
        },
        actix_web::http::Method::POST => {
            let payload = payload
                .ok_or_else(|| actix_web::error::ErrorBadRequest("Missing request payload"))?;

            match query.q.as_str() {
                "upload" => match payload {
                    web::Either::Right(multipart) => {
                        Ok(VueFinder::upload(data, query, multipart).await)
                    }
                    _ => Err(actix_web::error::ErrorBadRequest(
                        "Upload requests should use multipart/form-data",
                    )),
                },
                cmd @ ("newfile" | "rename" | "move" | "delete" | "save" | "archive"
                | "unarchive") => match payload {
                    web::Either::Left(json) => match cmd {
                        "newfile" => {
                            let payload: NewFileRequest = serde_json::from_value(json.into_inner())
                                .map_err(actix_web::error::ErrorBadRequest)?;
                            Ok(VueFinder::new_file(data, query, web::Json(payload)).await)
                        }
                        "rename" => {
                            let payload: RenameRequest = serde_json::from_value(json.into_inner())
                                .map_err(actix_web::error::ErrorBadRequest)?;
                            Ok(VueFinder::rename(data, query, web::Json(payload)).await)
                        }
                        "move" => {
                            let payload: MoveRequest = serde_json::from_value(json.into_inner())
                                .map_err(actix_web::error::ErrorBadRequest)?;
                            Ok(VueFinder::r#move(data, query, web::Json(payload)).await)
                        }
                        "delete" => {
                            let payload: DeleteRequest = serde_json::from_value(json.into_inner())
                                .map_err(actix_web::error::ErrorBadRequest)?;
                            Ok(VueFinder::delete(data, query, web::Json(payload)).await)
                        }
                        "save" => {
                            let payload: SaveRequest = serde_json::from_value(json.into_inner())
                                .map_err(actix_web::error::ErrorBadRequest)?;
                            Ok(VueFinder::save(data, query, web::Json(payload)).await)
                        }
                        "archive" => {
                            let payload: ArchiveRequest = serde_json::from_value(json.into_inner())
                                .map_err(actix_web::error::ErrorBadRequest)?;
                            Ok(VueFinder::archive(data, query, web::Json(payload)).await)
                        }
                        "unarchive" => {
                            let payload: UnarchiveRequest =
                                serde_json::from_value(json.into_inner())
                                    .map_err(actix_web::error::ErrorBadRequest)?;
                            Ok(VueFinder::unarchive(data, query, web::Json(payload)).await)
                        }
                        _ => unreachable!(),
                    },
                    _ => Err(actix_web::error::ErrorBadRequest("Expected JSON payload")),
                },
                _ => Ok(HttpResponse::BadRequest().finish()),
            }
        }
        _ => Ok(HttpResponse::MethodNotAllowed().finish()),
    }
}
