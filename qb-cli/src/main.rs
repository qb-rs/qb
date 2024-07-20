use std::time::Duration;

use interprocess::local_socket::{traits::tokio::Stream, GenericNamespaced, ToNsName};

#[tokio::main]
async fn main() {
    let name = "qb-daemon.sock";
    let name = name.to_ns_name::<GenericNamespaced>().unwrap();

    let _connection = match interprocess::local_socket::tokio::Stream::connect(name).await {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("could not connect to daemon socket: {}", err);
            return;
        }
    };

    println!("connected to daemon!");

    tokio::time::sleep(Duration::from_secs(5)).await;
}
