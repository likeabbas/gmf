use axum::{
    routing::get,
    Router,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::net::SocketAddr;
use hyper::service::service_fn;
use gmf::server::gmf_server::GmfServer;

async fn hello_world() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "message": "Hello, world!" })))
}

fn main() {
    // Initialize logging
    env_logger::init();

    // Define the app routes
    let app = Router::new().route("/", get(hello_world));

    // Define the socket address for the server
    let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();

    // Create a service that can handle Axum's requests
    let service = service_fn(move |req| {
        let app = app.clone();
        async move {
            Ok::<_, hyper::Error>(app.oneshot(req).await.unwrap())
        }
    });

    // Create and run the Glommio-based server
    let gmf = GmfServer::new(service, 10240);
    gmf.serve(addr).unwrap_or_else(|e| panic!("failed {}", e));
}
