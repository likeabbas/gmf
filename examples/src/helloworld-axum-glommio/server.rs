use axum::{
    routing::get,
    Router,
    extract::Request,
    http::{StatusCode, Method, Uri, Version},
    response::IntoResponse,
    Json,
    body::{HttpBody, Full},
};
use serde_json::json;
use std::net::SocketAddr;
use hyper::{service::service_fn, Request as HyperRequest, Body, http::{HeaderMap as HyperHeaderMap, Extensions as HyperExtensions}};
use hyper::body::Incoming;
use gmf::server::gmf_server::GmfServer;
use tower::{Service, ServiceExt};
use bytes::Bytes;

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
    let service = hyper::service::service_fn(move |req: Request<Incoming>| {
        let app = app.clone();
        async move {
            app.clone().call(req)
        }
        // let app = app.clone();
        // async move {
            // // Convert Hyper request to Axum request
            // let (mut parts, body) = req.into_parts();
            // let body_bytes = hyper::body::to_bytes(body).await.unwrap();
            // let axum_body = Full::from(Bytes::from(body_bytes));
            //
            // // Manually convert Hyper parts to Axum parts
            // let mut axum_extensions = AxumExtensions::new();
            // std::mem::swap(parts.extensions_mut(), &mut axum_extensions);
            //
            // let axum_headers = parts.headers.into_iter().collect::<AxumHeaderMap>();
            //
            // let axum_parts = axum::http::request::Parts {
            //     method: parts.method,
            //     uri: Uri::from_maybe_shared(Bytes::from(parts.uri.to_string())).unwrap(),
            //     version: parts.version,
            //     headers: axum_headers,
            //     extensions: axum_extensions,
            //     _priv: (),
            // };
            //
            // let axum_req = AxumRequest::from_parts(axum_parts, axum_body);
            //
            // // Now you can use `oneshot` with the Axum request
            // Ok::<_, hyper::Error>(app.oneshot(axum_req).await.unwrap())

        // }
    });

    // Create and run the Glommio-based server
    let gmf = GmfServer::new(service, 10240);
    gmf.serve(addr).unwrap_or_else(|e| panic!("failed {}", e));
}
