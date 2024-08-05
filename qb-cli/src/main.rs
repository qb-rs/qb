use core::panic;

use clap::{Parser, Subcommand, ValueEnum};
use interprocess::local_socket::{traits::tokio::Stream, GenericNamespaced, ToNsName};
use qb_control::{QBControlRequest, QBControlResponse};
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
        #[arg(long="id", value_parser=parse_id)]
        id: QBID,
        /// the type of QBI
        kind: Kind,
    },
    /// Detach a QBI
    Detach {
        /// the id of the QBI in hex format
        #[arg(long="id", value_parser=parse_id)]
        id: QBID,
    },
    /// Send a message to a QBI
    Bridge {
        /// the id of the QBI in hex format
        #[arg(long="id", value_parser=parse_id)]
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

type Len = u64;
const LEN_SIZE: usize = std::mem::size_of::<Len>();
const READ_SIZE: usize = 64;

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
            write(&mut conn, req).await;
            let res = read(&mut conn).await;

            println!("res: {}", res);
        }
        Commands::Detach { id } => {
            let req = QBControlRequest::Detach { id };
            let mut conn = connect().await;
            write(&mut conn, req).await;
        }
        Commands::Detach { id } => {
            let req = QBControlRequest::Detach { id };
            let mut conn = connect().await;
            write(&mut conn, req).await;
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

async fn write(conn: &mut TStream, req: QBControlRequest) {
    let contents = bitcode::encode(&req);
    let contents_len = contents.len() as Len;
    write_buf(conn, &contents_len.to_be_bytes()).await;
    write_buf(conn, &contents).await;
}

async fn read(conn: &mut TStream) -> QBControlResponse {
    let mut bytes: Vec<u8> = Vec::new();
    loop {
        if bytes.len() > LEN_SIZE {
            // read a message from the recv buffer
            let mut buf: [u8; LEN_SIZE] = [0; LEN_SIZE];
            buf.copy_from_slice(&bytes[0..LEN_SIZE]);
            let packet_len = LEN_SIZE + Len::from_be_bytes(buf) as usize;
            if packet_len > buf.len() {
                let packet = bytes.drain(0..packet_len).collect::<Vec<_>>();
                return bitcode::decode::<QBControlResponse>(&packet[LEN_SIZE..]).unwrap();
            }
        }

        let mut read_buf: [u8; READ_SIZE] = [0; READ_SIZE];
        let size = conn.read(&mut read_buf).await.unwrap();
        if size == 0 {
            panic!("remote closed the connection while reading");
        }
        bytes.extend_from_slice(&read_buf[0..size]);
    }
}

async fn write_buf(conn: &mut TStream, buf: &[u8]) {
    let mut written = 0;
    while written < buf.len() {
        written += conn.write(&buf[written..]).await.unwrap();
    }
}
