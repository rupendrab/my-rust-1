use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use clap::Parser;

mod cache;
pub use cache::Process;
pub use cache::MyCache;
pub use cache::read_process;

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

fn handle_refresh_cache() {
    if MyCache::should_refresh_cache() {
        MyCache::refresh_cache();
    }
}

async fn get_json_value(query: web::Query<QueryParams>) -> impl Responder {
    handle_refresh_cache();
    match read_process(&query.key) {
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let server_str = format!("{}:{}", "127.0.0.1", args.port);

    MyCache::refresh_cache();

    println!("Using port: {}", args.port);    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(get_json_value))
    })
    .bind(&server_str)?
    .run()
    .await
}
