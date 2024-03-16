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
    process_name: String,
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

#[derive(Serialize, Deserialize)]
struct ProcessPatchInput {
    name: String,
    run: Option<bool>,
    tags: Option<Vec<String>>
}

impl ProcessPatchInput {
    fn validatePatch(&self) -> bool {
        // Check that either `run` or `tags` is provided
        self.run.is_some() || self.tags.is_some()
    }
}

#[derive(Deserialize)]
pub struct DeleteProcessInput {
    process_name: String,
}

async fn get_json_value(data: web::Data<Arc<Mutex<MyCache>>>, query: web::Query<QueryParams>) -> impl Responder {
    let mut data = data.lock().await;
    data.refresh_cache(false).await;
    match data.get_process(&query.process_name) {
        Some(p) => HttpResponse::Ok().json(p),
        None => {
            let error_response = ErrorResponse {
                name: query.process_name.clone(),
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

async fn update_process_endpoint(data: web::Json<ProcessInput>, state: web::Data<Arc<Mutex<MyCache>>>) -> HttpResponse {
    let mut state = state.lock().await;
    let process = data.into_inner();
    let process_new = cache::create_process(&process.name, process.run, process.tags.clone());
    match state.modify_process(process_new.clone()).await {
        Ok(_) => {
            HttpResponse::Accepted().json("Process updated successfully!")
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

async fn delete_process_endpoint(query: web::Query<DeleteProcessInput>, state: web::Data<Arc<Mutex<MyCache>>>) -> impl Responder {
    let mut state = state.lock().await;
    let process_name = &query.process_name;
    info!("Deleting process: {}", process_name);
    match state.delete_process(&process_name).await {
        Ok(_) => {
            HttpResponse::Ok().json(format!("Process {} deleted successfully", process_name))
        }
        Err(e) => {
            let error_response = GenericErrorResponse { 
                code: 500, 
                message: format!("Failed to delete process: {}", e)
            };
            HttpResponse::InternalServerError().json(error_response)
        }
    }
}

async fn patch_process_endpoint(input: web::Json<ProcessPatchInput>, state: web::Data<Arc<Mutex<MyCache>>>) -> impl Responder {
    let mut state = state.lock().await;
    if !input.validatePatch() {
        return HttpResponse::InternalServerError().json("Either 'run' or 'tags' must be specified.");
    }
    info!("Patching process: {}", input.name);
    match state.update_process_partial(&input.name, input.run, input.tags.clone()).await {
        Ok(_) => {
            HttpResponse::Ok().json("Process patched successfully.")
        }
        Err(e) => {
            let error_response = GenericErrorResponse { 
                code: 500, 
                message: format!("Failed to patch process: {}", e)
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
            .service(
                web::resource("/process")
                    .route(web::get().to(get_json_value))
                    .route(web::post().to(add_process_endpoint))
                    .route(web::put().to(update_process_endpoint))
                    .route(web::delete().to(delete_process_endpoint))
                    .route(web::patch().to(patch_process_endpoint)),
            )
    })
    .bind(&server_str)?
    .run()
    .await
}
