use std::{
    env,
    sync::{mpsc, Arc, Mutex},
    thread,
};

// Config struct to hold server configuration
pub struct Config {
    pub address: String,
    pub base_dir: String,
    pub index_file: String,
    pub thread_count: usize,
}

impl Config {
    pub fn new() -> Self {
        let thread_count = env::var("THREADS")
            .unwrap_or_else(|_| "2".to_string())
            .parse()
            .unwrap_or(2)
            .max(1); // Ensure at least 1 thread

        Self {
            address: env::var("ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            base_dir: env::var("DIR").unwrap_or_else(|_| "./www".to_string()),
            index_file: env::var("INDEX").unwrap_or_else(|_| "index.html".to_string()),
            thread_count,
        }
    }
}

// Define Job type
type Job = Box<dyn FnOnce() + Send + 'static>;

// ThreadPool struct to hold workers and sender
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>, // Option to allow proper Drop handling
}

impl ThreadPool {
    pub fn new(size: usize) -> Self {
        assert!(size > 0, "Thread pool size must be greater than 0");
        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));

        let workers = (0..size)
            .map(|id| Worker::new(id, Arc::clone(&receiver)))
            .collect();

        Self {
            workers,
            sender: Some(sender),
        }
    }

    pub fn execute<F>(&self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if let Some(sender) = &self.sender {
            if sender.send(Box::new(job)).is_err() {
                eprintln!("Failed to send job: receiver may be closed");
            }
        }
    }
}

// Drop implementation for ThreadPool (graceful shutdown)
impl Drop for ThreadPool {
    fn drop(&mut self) {
        // Drop sender first to ensure workers exit
        self.sender.take();
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                if let Err(e) = thread.join() {
                    eprintln!("Failed to join worker thread: {:?}", e);
                }
            }
        }
    }
}

// Worker struct
pub struct Worker {
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Self {
        let thread = thread::Builder::new()
            .name(format!("worker-{}", id))
            .spawn(move || loop {
                let job = receiver.lock().unwrap().recv();
                match job {
                    Ok(task) => {
                        println!("Worker {} executing a job", id);
                        task();
                    }
                    Err(_) => {
                        println!("Worker {} shutting down", id);
                        break;
                    }
                }
            })
            .expect("Failed to spawn worker thread");

        Self {
            thread: Some(thread),
        }
    }
}
