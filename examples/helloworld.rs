use aruntime::executor::Executor;

fn main() {
    let ex = Executor::new();
    ex.block_on(|| async {
        println!("Hello, world!");
    });
}