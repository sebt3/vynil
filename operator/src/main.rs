#![allow(unused_imports, unused_variables)]
pub use controller::*;
use tracing_subscriber::{EnvFilter, Registry, prelude::*};

use actix_web::{
    App, HttpRequest, HttpResponse, HttpServer, Responder, get, middleware,
    web::{self, Data},
};

#[get("/metrics")]
async fn metrics(c: Data<Manager>, _req: HttpRequest) -> impl Responder {
    let metrics = c.metrics();
    HttpResponse::Ok()
        .content_type("application/openmetrics-text; version=1.0.0; charset=utf-8")
        .body(metrics)
}

#[get("/health")]
async fn health(_: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/")]
async fn index(c: Data<Manager>, _req: HttpRequest) -> impl Responder {
    let d = c.diagnostics().await;
    HttpResponse::Ok().json(&d)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup tracing layers
    #[cfg(feature = "telemetry")]
    let telemetry = tracing_opentelemetry::layer().with_tracer(telemetry::init_tracer().await);
    let logger = tracing_subscriber::fmt::layer();
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    // Decide on layers
    #[cfg(feature = "telemetry")]
    let collector = Registry::default().with(telemetry).with(logger).with(env_filter);
    #[cfg(not(feature = "telemetry"))]
    let collector = Registry::default().with(logger).with(env_filter);

    // Initialize tracing
    tracing::subscriber::set_global_default(collector).unwrap();

    common::context::init_k8s();
    // Start kubernetes controller
    let (manager, controller_jbs, controller_tnts, controller_stms, controller_svcs) = Manager::new().await;

    // Start web server
    let server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(manager.clone()))
            .wrap(middleware::Logger::default().exclude("/health"))
            .service(index)
            .service(health)
            .service(metrics)
    })
    .bind("0.0.0.0:9000")
    .expect("Can not bind to 0.0.0.0:9000")
    .shutdown_timeout(5);

    tokio::select! {
        _ = controller_jbs => tracing::warn!("JukeBox controller exited"),
        _ = controller_tnts => tracing::warn!("TenantInstance controller exited"),
        _ = controller_stms => tracing::warn!("SystemInstance controller exited"),
        _ = controller_svcs => tracing::warn!("ServiceInstance controller exited"),
        _ = server.run() => tracing::info!("actix exited"),
    }
    Ok(())
}
