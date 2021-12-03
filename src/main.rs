#![feature(hash_drain_filter)]

mod error;
pub mod image_manager;

use std::ops::Deref;

use actix_web::{middleware::Logger, web, App, HttpResponse, HttpServer};
use error::Result;
use futures_util::stream::StreamExt as _;
use image_manager::ImageManager;
use jemallocator::Jemalloc;
use uuid::Uuid;

#[global_allocator]
#[cfg(not(target_os = "windows"))]
static GLOBAL: Jemalloc = Jemalloc;

async fn index() -> HttpResponse {
    HttpResponse::Ok().body("Hello, world!")
}

async fn get(im: web::Data<ImageManager>, uuid: web::Path<Uuid>) -> Result<HttpResponse> {
    let res = im.get(uuid.into_inner()).await?;
    let data = res.deref().clone();
    Ok(HttpResponse::Ok().content_type(mime::IMAGE_JPEG).body(data))
}

async fn post(
    im: web::Data<ImageManager>,
    uuid: web::Path<Uuid>,
    mut body: web::Payload,
) -> Result<HttpResponse> {
    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    im.insert(uuid.into_inner(), bytes.to_vec()).await?;
    Ok(HttpResponse::Ok().finish())
}

async fn update_cache(
    im: web::Data<ImageManager>,
    uuids: web::Json<Vec<Uuid>>,
) -> Result<HttpResponse> {
    im.load_cache(uuids.into_inner()).await?;
    Ok(HttpResponse::Ok().finish())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().expect("Can't open .env");
    pretty_env_logger::init_timed();
    let address = std::env::var("SERVER_ADDRESS").unwrap();

    let im = ImageManager::new();

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(web::Data::new(im.clone()))
            .route("/", web::get().to(index))
            .route("/images/{uuid}", web::post().to(post))
            .route("/images/{uuid}", web::get().to(get))
            .route("/api/update_cache", web::post().to(update_cache))
    })
    .bind(address)?
    .run()
    .await
}
