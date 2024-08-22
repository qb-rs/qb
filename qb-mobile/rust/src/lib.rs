use flutter_rust_bridge::frb;
use tracing::{info, level_filters::LevelFilter};
use tracing_panic::panic_hook;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

pub mod api;
mod frb_generated;

#[frb(init)]
pub fn init_lib() {
    std::panic::set_hook(Box::new(panic_hook));

    let stdout_log = tracing_subscriber::fmt::layer().pretty();
    tracing_subscriber::registry()
        .with(stdout_log.with_filter(LevelFilter::DEBUG))
        .init();

    info!("Initializing rust library...");
}

pub mod android;
