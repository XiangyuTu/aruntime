use aruntime::{
    executor::Executor,
    net::TcpListener,
};

fn main() {
    let ex = Executor::new();
    ex.block_on(|| async {
        let listen = TcpListener::bind(("127.0.0.1", 30000)).unwrap();
        while let Ok((stream, _)) = listen.accept().await {
            let f = async move {
                let mut buf = [0u8; 1024];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(n) => {
                            stream.write(&buf[..n]).await.unwrap();
                        }
                        Err(e) => {
                            println!("read err: {:?}", e);
                            break;
                        }
                    }
                }
            };
            Executor::spawn(f);
        }
    });
}