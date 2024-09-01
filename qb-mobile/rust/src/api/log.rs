use crate::frb_generated::StreamSink;
use std::sync::Arc;
use tracing::{info, level_filters::LevelFilter};
use tracing_panic::panic_hook;
use tracing_subscriber::{fmt::MakeWriter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[derive(Clone)]
struct LogWriter {
    sink: Arc<StreamSink<Vec<u8>>>,
}

impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.sink.add(buf.into()).unwrap();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Nothing to do here
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }

    fn make_writer_for(&'a self, _meta: &tracing::Metadata<'_>) -> Self::Writer {
        self.clone()
    }
}

pub fn init_log(log_sink: StreamSink<Vec<u8>>) {
    let make_writer = LogWriter {
        sink: Arc::new(log_sink),
    };

    let stdout_log = tracing_subscriber::fmt::layer()
        .pretty()
        .with_writer(make_writer);
    tracing_subscriber::registry()
        .with(stdout_log.with_filter(LevelFilter::TRACE))
        .init();
    std::panic::set_hook(Box::new(panic_hook));

    info!("logging initialized...");
}
