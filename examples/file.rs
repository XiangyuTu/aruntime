use aruntime::executor::Executor;
use aruntime::fs::File;
use std::os::fd::AsRawFd;

fn main() {
    let ex = Executor::new();
    ex.block_on(|| async {
        let file1 = File::open("/proc/cpuinfo");
        let file2 = File::open("/etc/hostname");

        println!("file1 fd: {}", file1.as_raw_fd());
        println!("file2 fd: {}", file2.as_raw_fd());

        let mut buf = [0u8; 1024*4];
        match file1.read(&mut buf).await {
            Ok(n) => {
                // to string
                let res_str = std::str::from_utf8(&buf[..n]).unwrap();

                println!("file1:\n{}", res_str);
            }
            Err(e) => {
                println!("file1 err: {:?}", e);
            }
        }

        let file3: File = std::fs::File::options().create(true).write(true).open("./test.txt").unwrap().into();
        
        println!("file3 fd: {}", file3.as_raw_fd());
        let buf = b"Hello, world!\n";

        match file3.write(buf).await {
            Ok(n) => {
                println!("file3 written: {:?}", n);
            }
            Err(e) => {
                println!("file3 err: {:?}", e);
            }
        }
    });
}