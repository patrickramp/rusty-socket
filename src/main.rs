mod handler;
mod structs;

use crate::handler::handle_client;
use crate::structs::{Config, ThreadPool};
use signal_hook::iterator::Signals;
use std::thread;
use std::time::Duration;
use std::{
    io,
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
};

fn main() -> io::Result<()> {
    // Load configuration
    let config = Config::new();
    let base_dir = Path::new(&config.base_dir).canonicalize()?;
    if !base_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Base directory not found",
        ));
    }

    // Create TCP listener with explicit binding
    let listener = TcpListener::bind(&config.address)?;
    listener.set_nonblocking(true)?; // Prevent blocking on slow clients

    // Print configuration
    println!("rusty-socket v0.1.1");
    println!("Opening rusty-socket on {}", config.address);
    println!("Base directory: {:?}", &base_dir);
    println!("Index file: {}", config.index_file);
    println!("Thread count: {}", config.thread_count);

    // Create a thread pool
    let pool = ThreadPool::new(config.thread_count);

    // Wrap shared data in Arc
    let base_dir = Arc::new(base_dir);
    let index_file = Arc::new(config.index_file);

    // Graceful shutdown flag
    let running = Arc::new(AtomicBool::new(true));

    // Handle SIGTERM for graceful shutdown
    let mut signals = Signals::new(&[signal_hook::consts::SIGTERM])?;
    let shutdown_flag = running.clone();
    thread::spawn(move || {
        for _ in signals.forever() {
            println!("\nReceived SIGTERM. Shutting down...");
            shutdown_flag.store(false, Ordering::Relaxed);
            break;
        }
    });

    // Handle incoming connections
    while running.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let base_dir = Arc::clone(&base_dir);
                let index_file = index_file.clone();
                pool.execute(move || handle_client(stream, base_dir, &index_file));
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100)); // Prevent busy loop
                continue;
            }
            Err(e) => eprintln!("Connection failed: {}", e),
        }
    }

    println!("Shutting down gracefully...");
    Ok(())
}
