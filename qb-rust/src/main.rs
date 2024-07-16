use std::{fs::File, sync::Arc, time::Duration};
use tracing_panic::panic_hook;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

use qb::interface::QBI;
use qbi_local::{QBILocal, QBILocalInit};

#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(panic_hook));

    let stdout_log = tracing_subscriber::fmt::layer().pretty();

    // A layer that logs events to a file.
    let file = File::create("debug.log").unwrap();
    let debug_log = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(Arc::new(file));

    tracing_subscriber::registry()
        .with(
            stdout_log
                .with_filter(filter::LevelFilter::INFO)
                .and_then(debug_log),
        )
        .init();

    let mut qb = qb::QB::init("./local").await;

    qb.attach_qbi(
        "local1",
        QBILocal::init,
        QBILocalInit {
            path: "./local1".into(),
        },
    )
    .await;

    //qb.attach_qbi(
    //    "local2",
    //    QBILocal::init,
    //    QBILocalInit {
    //        path: "./local2".into(),
    //    },
    //)
    //.await;

    //qb.changelog.push(QBChange::now(
    //    QBChangeKind::Create,
    //    unsafe { QBPath::new("abc.txt") }.file(),
    //));

    loop {
        qb.process_handles().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
