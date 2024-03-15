use actix_web::{web, App, HttpResponse, HttpServer, Responder, Result, Error};
use serde::{Deserialize, Serialize};
use clap::Parser;
use std::env;
use std::path::PathBuf;

mod cache;
pub use cache::Process;
pub use cache::MyCache;
pub use cache::read_process;
use std::sync::Arc;
use tokio::sync::Mutex;
use actix_files as fs;
use log::{debug, error, log_enabled, info, Level};

mod s3_util;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "3000")]
    port: String,
}

#[derive(Deserialize)]
struct QueryParams {
    key: String,
}

#[derive(Serialize, Deserialize)]
struct ErrorResponse {
    name: String,
    run: bool,
}

#[derive(Serialize, Deserialize)]
struct GenericErrorResponse {
    code: u32,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct ProcessInput {
    name: String,
    run: bool,
    tags: Option<Vec<String>>
}

async fn get_json_value(data: web::Data<Arc<Mutex<MyCache>>>, query: web::Query<QueryParams>) -> impl Responder {
    let mut data = data.lock().await;
    data.refresh_cache(false).await;
    match data.get_process(&query.key) {
        Some(p) => HttpResponse::Ok().json(p),
        None => {
            let error_response = ErrorResponse {
                name: query.key.clone(),
                run: true,
            };
            HttpResponse::NotFound().json(error_response)
        }
    }
}

async fn add_process_endpoint(data: web::Json<ProcessInput>, state: web::Data<Arc<Mutex<MyCache>>>) -> impl Responder {
    let mut state = state.lock().await;
    let process = data.into_inner();
    let process_new = cache::create_process(&process.name, process.run, process.tags.clone());
    match state.add_process(process_new.clone()).await {
        Ok(_) => {
            HttpResponse::Created().json(process_new)
        }
        Err(e) => {
            let error_response = GenericErrorResponse { 
                code: 500, 
                message: format!("Failed to add process: {}", e)
            };
            HttpResponse::InternalServerError().json(error_response)
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let args = Args::parse();

    let server_str = format!("{}:{}", "0.0.0.0", args.port);

    let client = s3_util::get_client().await.expect("Failed to get S3 client");
    cache::S3_CLIENT.set(client).expect("Failed to set S3_CLIENT");
    info!("Set up new S3 Client!");

    let (all_processes, etag) = cache::read_data().await.expect("Failed to read data");
    let cache_time = cache::get_current_time();
    let cached_data = MyCache {
        all_processes,
        cache_time,
        etag
    };
    let cached_data = Arc::new(Mutex::new(cached_data));

    info!("Using port: {}", args.port);    
    let docs_dir = env::var("DOCS_DIR").unwrap_or_else(|_| "./docs".to_string());
    let opanapi_file = format!("{}/openapi.html", docs_dir);

    HttpServer::new(move || {
        let openapi_file_for_route = opanapi_file.clone();

        App::new()
            .app_data(web::Data::new(cached_data.clone()))
            .service(fs::Files::new("/docs", &docs_dir).show_files_listing())
            // .route("/", web::get().to(|| async { fs::NamedFile::open("./docs/openapi.html").unwrap() }))
            .route("/", web::get().to(move || {
                let openapi_path = PathBuf::from(openapi_file_for_route.clone());
                async move {
                    fs::NamedFile::open(&openapi_path)
                        .map_err(Error::from) // This ensures the error is converted properly
                }
            }))
            .route("/process", web::get().to(get_json_value))
            .service(web::resource("/process").route(web::post().to(add_process_endpoint)))
    })
    .bind(&server_str)?
    .run()
    .await
}
