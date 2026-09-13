#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::arithmetic_side_effects, clippy::indexing_slicing))]
#![allow(unused_imports, unused_variables)]
pub use controller::*;
use tracing_subscriber::{EnvFilter, Registry, prelude::*};

use actix_web::{
    App, HttpRequest, HttpResponse, HttpServer, Responder, get, middleware,
    web::{self, Data},
};

#[get("/metrics")]
async fn metrics(c: Data<Manager>) -> impl Responder {
    let metrics = c.metrics();
    HttpResponse::Ok()
        .content_type("application/openmetrics-text; version=1.0.0; charset=utf-8")
        .body(metrics)
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/")]
async fn index(c: Data<Manager>) -> impl Responder {
    let d = c.diagnostics().await;
    HttpResponse::Ok().json(&d)
}

fn main() -> Result<()> {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024) // kube 2.0 + async_trait : state machines en debug sont larges
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => panic!("failed to build Tokio runtime: {e}"),
    };
    runtime.block_on(async_main())
}

async fn async_main() -> Result<()> {
    // rustls ne détecte plus automatiquement le CryptoProvider depuis les features du crate
    // (régression 0.23.41) : kube construit sa ClientConfig TLS dès la création du client et
    // paniquerait. On installe explicitement le provider ring.
    rustls::crypto::ring::default_provider().install_default().ok();

    // Setup tracing layers
    #[cfg(feature = "telemetry")]
    let telemetry = tracing_opentelemetry::layer().with_tracer(telemetry::init_tracer().await);
    let logger = tracing_subscriber::fmt::layer();
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // Decide on layers
    #[cfg(feature = "telemetry")]
    let collector = Registry::default().with(telemetry).with(logger).with(env_filter);
    #[cfg(not(feature = "telemetry"))]
    let collector = Registry::default().with(logger).with(env_filter);

    // Initialize tracing
    if let Err(e) = tracing::subscriber::set_global_default(collector) {
        panic!("failed to install the global tracing subscriber: {e}");
    }

    common::context::init_k8s();
    common::context::wire_core_k8s();
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
    .map_err(|e| Error::Other(format!("can not bind to 0.0.0.0:9000: {e}")))?
    .shutdown_timeout(5);

    tokio::select! {
        () = controller_jbs => tracing::warn!("JukeBox controller exited"),
        () = controller_tnts => tracing::warn!("TenantInstance controller exited"),
        () = controller_stms => tracing::warn!("SystemInstance controller exited"),
        () = controller_svcs => tracing::warn!("ServiceInstance controller exited"),
        _ = server.run() => tracing::info!("actix exited"),
    }
    Ok(())
}
