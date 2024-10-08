use std::{fs::File, sync::Arc};

use clap::{Parser, Subcommand};
use interprocess::local_socket::{traits::tokio::Stream, GenericNamespaced, ToNsName};
use qb_ext::{
    control::{QBCRequest, QBCResponse},
    QBExtId,
};
use qb_proto::{QBPBlob, QBP};
use tokio::io::AsyncReadExt;
use tracing_panic::panic_hook;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt, Layer};

type TStream = interprocess::local_socket::tokio::Stream;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Subcommand
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List the connected extensions
    List,
    /// Add an extension
    Add {
        /// The name of the extension kind ("gdrive", "local", ...)
        name: String,
        #[arg(long = "type", default_value = "application/json")]
        content_type: String,
        content: Option<String>,
    },
    #[command(name = "rm")]
    /// Remove an extension
    Remove {
        /// the id of the extension in hex format
        #[arg(value_parser=parse_id)]
        id: QBExtId,
    },
    /// Start an extension
    Start {
        /// the id of the extension in hex format
        #[arg(value_parser=parse_id)]
        id: QBExtId,
    },
    /// Stop an extension
    Stop {
        /// the id of the extension in hex format
        #[arg(value_parser=parse_id)]
        id: QBExtId,
    },
}

fn parse_id(s: &str) -> Result<QBExtId, String> {
    QBExtId::from_hex(s).map_err(|e| e.to_string())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Cli::parse();

    std::panic::set_hook(Box::new(panic_hook));

    let stdout_log = tracing_subscriber::fmt::layer().pretty();

    // A layer that logs events to a file.
    let file = File::create("/tmp/qb-cli.log").unwrap();
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

    process_args(args).await;
}

async fn process_args(args: Cli) -> Option<()> {
    match args.command {
        Commands::Add {
            name,
            content_type,
            content,
        } => {
            let content = match content {
                Some(content) => content.into_bytes(),
                None => {
                    let mut buf = Vec::new();
                    tokio::io::stdin().read_to_end(&mut buf).await.unwrap();
                    buf
                }
            };
            let req = QBCRequest::Add {
                blob: QBPBlob {
                    content_type,
                    content,
                },
                name,
            };

            let mut conn = connect().await?;
            let mut protocol = QBP::default();
            protocol.negotiate(&mut conn).await.unwrap();
            protocol.send(&mut conn, req).await.unwrap();
            finish(protocol, conn).await;
        }
        Commands::Remove { id } => {
            let req = QBCRequest::Remove { id };
            let mut conn = connect().await?;
            let mut protocol = QBP::default();
            protocol.negotiate(&mut conn).await.unwrap();
            protocol.send(&mut conn, req).await.unwrap();
            finish(protocol, conn).await;
        }
        Commands::Start { id } => {
            let req = QBCRequest::Start { id };
            let mut conn = connect().await?;
            let mut protocol = QBP::default();
            protocol.negotiate(&mut conn).await.unwrap();
            protocol.send(&mut conn, req).await.unwrap();
            finish(protocol, conn).await;
        }
        Commands::Stop { id } => {
            let req = QBCRequest::Stop { id };
            let mut conn = connect().await?;
            let mut protocol = QBP::default();
            protocol.negotiate(&mut conn).await.unwrap();
            protocol.send(&mut conn, req).await.unwrap();
            finish(protocol, conn).await;
        }
        Commands::List => {
            let req = QBCRequest::List;
            let mut conn = connect().await?;
            let mut protocol = QBP::default();
            protocol.negotiate(&mut conn).await.unwrap();
            protocol.send(&mut conn, req).await.unwrap();
            finish(protocol, conn).await;
        }
    };

    Some(())
}

async fn finish(mut protocol: QBP, mut conn: TStream) {
    let resp = protocol.recv::<QBCResponse>(&mut conn).await.unwrap();
    match resp {
        QBCResponse::Error { .. } => eprintln!("{}", resp),
        _ => println!("{}", resp),
    }
}

async fn connect() -> Option<TStream> {
    let name = "qb-daemon.sock";
    let name = name.to_ns_name::<GenericNamespaced>().unwrap();

    let connection = match TStream::connect(name).await {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("could not connect to daemon socket: {}", err);
            return None;
        }
    };

    Some(connection)
}
