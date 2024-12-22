use aruntime::{
    executor::Executor,
    net::TcpListener,
};

fn main() {
    let ex = Executor::new();
    ex.block_on(|| async {
        let listen = TcpListener::bind(("127.0.0.1", 30000)).unwrap();
        loop {
            let _ = listen.accept().await;
            println!("accept");
        }
    });
}