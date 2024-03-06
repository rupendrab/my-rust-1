use reqwest::{Error, Response, StatusCode};
use rand::seq::SliceRandom; // For the choose method
use rand::thread_rng; // Provides a random number generator
use tokio::time::{Instant, Duration};
use std::sync::Arc;
use tokio::sync::Mutex;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Number of API Calls
    #[arg(short, long)]
    calls: usize,

    /// Number of parallel threads
    #[arg(short, long, default_value_t = 20)]
    threads: usize,
}

async fn call_api(process: &str) -> u128 {
    let start = Instant::now();
    let url = format!("http://localhost:3000/?key={}", process);
    match get_http_response(&url).await {
        Ok((status, body)) => {
            println!("Status: {}, Body: {}", status, body);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
    let elapsed = start.elapsed().as_millis();
    elapsed
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    let args = Args::parse();
    let no_calls = args.calls;
    let threads = args.threads;

    let start = Instant::now();
    
    let processes = Arc::new(get_sample_processes(&get_list_of_processes(), no_calls as usize));
    let mut tasks = Vec::new();

    // Assuming each thread should process a part of the total list (e.g., 20 processes per thread)
    let chunk_size = processes.len() / threads; // Calculate chunk size for even distribution

    for i in 0..threads {
        // Create a slice for each chunk to process
        let process_slice = Arc::clone(&processes)[i * chunk_size..(i+1) * chunk_size].to_vec();
        
        // Spawn a new async task for each slice
        let handle = tokio::spawn(async move {
            let mut durations = Vec::new();
            for process in process_slice {
                durations.push(call_api(&process).await);
            }
            durations
        });

        tasks.push(handle);
    }

    // Wait for all tasks to complete
    let mut all_durations = Vec::new(); 
    for task in tasks {
        let slice_durations = task.await.expect("Task panicked");
        all_durations.extend(slice_durations);
    }

    // Calculate the average
    let sum: u128 = all_durations.iter().sum();
    let count = all_durations.len() as u128; // Safe cast, Vec::len() returns usize
    let average = if count > 0 { sum / count } else { 0 };

    /* 
    for process in get_sample_processes(&get_list_of_processes(), 20) {
        call_api(&process).await;
    }
    */

    let duration = start.elapsed();
    println!("Time elapsed in calling the API {} times with {} parallel calls: {:?}", no_calls, threads, duration);
    println!("Average time per individual call: {} ms", average);
}

fn get_list_of_processes() -> Vec<String> {
    let numbers = (1..=100).map(|i| format!("process{}", i));
    let letters = ('a'..='z').map(|c| format!("process{}", c));
    numbers.chain(letters).collect()
}

fn get_sample_processes(orig_list: &Vec<String>, sample_size: usize) -> Vec<String> {
    let mut rng = rand::thread_rng();
    let mut sample = Vec::with_capacity(sample_size as usize);
    for _ in 0..sample_size {
        if let Some(item) = orig_list.choose(&mut rng) {
            sample.push(item.clone());
        }
    }
    sample
}

async fn get_http_response(url: &str) -> Result<(StatusCode, String), Error> {
    let client = reqwest::Client::new();
    
    let res = client.get(url)
        .send()
        .await?;
    
    let status = res.status();
    let body = res.text().await?;
    
    Ok((status, body))
}
