mod cache;
pub use cache::Process;
pub use cache::MyCache;
pub use cache::read_process;

fn main() {
    MyCache::refresh_cache();

    match read_process("process1") {
        Some(p) => println!("Process found: {:?}", p),
        None => println!("Process not found")
    }
    
    MyCache::refresh_cache();

    match read_process("process2") {
        Some(p) => println!("Process found: {:?}", p),
        None => println!("Process not found")
    }

    MyCache::add_process(Process {
        name: "processx".to_string(),
        run: true,
        effective: "2024-11-18T09:34:41".to_string()
    });
    println!("Process added!");

    match read_process("processx") {
        Some(p) => println!("Process found: {:?}", p),
        None => println!("Process not found")
    }

    MyCache::modify_process(Process {
        name: "processx".to_string(),
        run: false,
        effective: "2024-11-18T09:34:41".to_string()
    });
    println!("Process modified!");
    
    match read_process("processx") {
        Some(p) => println!("Process found: {:?}", p),
        None => println!("Process not found")
    }

    MyCache::write_cache();
}

