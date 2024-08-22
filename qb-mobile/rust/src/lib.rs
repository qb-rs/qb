use flutter_rust_bridge::frb;
use tracing_panic::panic_hook;

mod android;
pub mod api;
mod frb_generated;

#[frb(init)]
pub fn init_lib() {
    std::panic::set_hook(Box::new(panic_hook));
}
