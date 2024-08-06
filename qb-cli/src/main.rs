use core::panic;

use clap::{Parser, Subcommand, ValueEnum};
use interprocess::local_socket::{traits::tokio::Stream, GenericNamespaced, ToNsName};
use qb_control::{QBControlRequest, QBControlResponse};
use qb_core::interface::QBIId;
use qb_proto::QBP;
use tokio::io::AsyncReadExt;

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
    /// List the connected QBIs
    List,
    Setup {
        name: String,
        #[arg(long = "type", default_value = "application/json")]
        content_type: String,
    },
    Start {
        /// the id of the QBI in hex format
        #[arg(long="id", value_parser=parse_id)]
        id: QBIId,
    },
    Stop {
        /// the id of the QBI in hex format
        #[arg(long="id", value_parser=parse_id)]
        id: QBIId,
    },
    /// Send a message to a QBI
    Bridge {
        /// the id of the QBI in hex format
        #[arg(long="id", value_parser=parse_id)]
        id: QBIId,
        /// the message (will read from stdin if left blank)
        msg: Option<String>,
    },
}

#[derive(Clone, ValueEnum)]
enum Kind {
    /// qbi-local
    Local,
}

fn parse_id(s: &str) -> Result<QBIId, String> {
    QBIId::from_hex(s).map_err(|e| e.to_string())
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    match args.command {
        Commands::Bridge { id, msg } => {
            let msg = match msg {
                Some(msg) => msg.into_bytes(),
                None => {
                    let mut buf = Vec::new();
                    tokio::io::stdin().read_to_end(&mut buf).await.unwrap();
                    buf
                }
            };
            let req = QBControlRequest::Bridge { id, msg };
            let mut conn = connect().await;
            let mut protocol = QBP::default();
            protocol.negotiate(&mut conn).await.unwrap();
            protocol.send(&mut conn, req).await.unwrap();
            let res = protocol.read::<QBControlResponse>(&mut conn).await.unwrap();

            println!("res: {}", res);
        }
        Commands::Setup { name, content_type } => {
            let mut content = Vec::new();
            tokio::io::stdin().read_to_end(&mut content).await.unwrap();
            let req = QBControlRequest::Setup { content_type, name };

            let mut conn = connect().await;
            let mut protocol = QBP::default();
            protocol.negotiate(&mut conn).await.unwrap();
            protocol.send(&mut conn, req).await.unwrap();
            protocol.send_payload(&mut conn, &content).await.unwrap();
        }
        Commands::Start { id } => {
            let req = QBControlRequest::Start { id };

            let mut conn = connect().await;
            let mut protocol = QBP::default();
            protocol.negotiate(&mut conn).await.unwrap();
            protocol.send(&mut conn, req).await.unwrap();
        }
        Commands::Stop { id } => {
            let req = QBControlRequest::Stop { id };
            let mut conn = connect().await;
            let mut protocol = QBP::default();
            protocol.negotiate(&mut conn).await.unwrap();
            protocol.send(&mut conn, req).await.unwrap();
        }
        _ => unimplemented!(),
    };
}

async fn connect() -> TStream {
    let name = "qb-daemon.sock";
    let name = name.to_ns_name::<GenericNamespaced>().unwrap();

    let connection = match TStream::connect(name).await {
        Ok(conn) => conn,
        Err(err) => {
            panic!("could not connect to daemon socket: {}", err);
        }
    };

    println!("connected to daemon!");

    connection
}
