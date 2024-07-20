use std::{io::Read, time::Duration};

use clap::{Parser, Subcommand, ValueEnum};
use interprocess::local_socket::{traits::tokio::Stream, GenericNamespaced, ToNsName};
use qb_control::QBControlRequest;
use qb_core::common::id::QBID;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    /// Attach a QBI
    Attach {
        /// the id of the QBI in hex format
        #[arg(value_parser=parse_id)]
        id: QBID,
        /// the type of QBI
        kind: Kind,
    },
    /// Detach a QBI
    Detach {
        /// the id of the QBI in hex format
        #[arg(value_parser=parse_id)]
        id: QBID,
    },
    /// Send a message to a QBI
    Bridge {
        /// the id of the QBI in hex format
        #[arg(value_parser=parse_id)]
        id: QBID,
        /// the message (will read from stdin if left blank)
        msg: Option<String>,
    },
}

#[derive(Clone, ValueEnum)]
enum Kind {
    /// qbi-local
    Local,
}

fn parse_id(s: &str) -> Result<QBID, String> {
    QBID::from_hex(s).map_err(|e| e.to_string())
}

type LEN = u64;
const LEN_SIZE: usize = std::mem::size_of::<LEN>();

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
            let contents = bitcode::encode(&req);
            let contents_len = contents.len() as LEN;
            let mut conn = connect().await;
            conn.write(&contents_len.to_be_bytes()).await.unwrap();
            conn.write(&contents).await.unwrap();

            tokio::time::sleep(Duration::from_secs(1)).await;
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
