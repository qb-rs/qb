use interprocess::local_socket::{traits::tokio::Stream, GenericNamespaced, ToNsName};

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    let name = "qb-daemon.sock";
    let name = name.to_ns_name::<GenericNamespaced>().unwrap();

    let _connection = interprocess::local_socket::tokio::Stream::connect(name)
        .await
        .unwrap();
    println!("Connected to deamon!");
}
