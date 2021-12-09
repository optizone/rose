#![feature(hash_drain_filter)]

mod error;
pub mod fs_manager;

use actix_web::{middleware::Logger, web, App, HttpResponse, HttpServer};
use error::Result;
use fs_manager::FsManager;
use futures_util::stream::StreamExt;
use jemallocator::Jemalloc;
use uuid::Uuid;

#[global_allocator]
#[cfg(not(target_os = "windows"))]
static GLOBAL: Jemalloc = Jemalloc;

async fn index() -> HttpResponse {
    HttpResponse::Ok().body("Hello, world!")
}

async fn get(im: web::Data<FsManager>, uuid: web::Path<Uuid>) -> Result<HttpResponse> {
    let data = im.get(uuid.into_inner(), "jpeg").await?;
    Ok(HttpResponse::Ok()
        .content_type(mime::IMAGE_JPEG)
        .streaming(data))
}

async fn post(
    im: web::Data<FsManager>,
    uuid: web::Path<Uuid>,
    body: web::Payload,
) -> Result<HttpResponse> {
    im.insert(
        uuid.into_inner(),
        body.map(|b| b.map_err(|e| e.into())),
        "jpeg",
    )
    .await?;
    Ok(HttpResponse::Ok().finish())
}

async fn update_cache(
    im: web::Data<FsManager>,
    uuids: web::Json<Vec<(Uuid, String)>>,
) -> Result<HttpResponse> {
    let uuids = uuids.into_inner();
    let uuids = uuids.iter().map(|(id, ext)| (*id, ext.as_str())).collect();
    im.load_cache(uuids).await?;
    Ok(HttpResponse::Ok().finish())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().expect("Can't open .env");
    pretty_env_logger::init_timed();
    let address = std::env::var("SERVER_ADDRESS").unwrap();

    let im = FsManager::from_env();

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(web::Data::new(im.clone()))
            .route("/", web::get().to(index))
            .route("/images/{uuid}", web::post().to(post))
            .route("/images/{uuid}", web::get().to(get))
            .route("/api/force_load_cache", web::post().to(update_cache))
    })
    .bind(address)?
    .run()
    .await
}
