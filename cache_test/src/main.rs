use once_cell::sync::Lazy;
use std::sync::Mutex;

use std::fs::File;
use std::io::{BufReader};
use serde::{Deserialize, Serialize};
use serde_json::{from_reader};
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Process {
  name: String,
  run: bool,
  effective: String, // Store effective date/time as a string for simplicity
}

struct MyCache {
    all_processes: Vec<Process>,
}

fn read_data() -> Result<Vec<Process>> {
    let file = File::open("processes.json")?;
    let reader = BufReader::new(file);
  
    // Read the JSON data from the file
    let data: Vec<Process> = from_reader(reader)?;
    Ok(data)
}

fn update_data(data: Vec<Process>) -> Result<()> {
    let file = File::create("processes.json")?;
    serde_json::to_writer(file, &data)?;
    Ok(())
}

impl MyCache {
    fn get_instance() -> &'static Lazy<Mutex<MyCache>> {
        static INSTANCE: Lazy<Mutex<MyCache>> = Lazy::new(|| {
            let data = match read_data() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error reading cache: {:?}", e);
                    std::process::exit(1);
                }
            };
            Mutex::new(MyCache {
                all_processes: data
            })
        });
        &INSTANCE
    }

    pub fn write_cache() {
        let cache = Self::get_instance().lock().expect("Failed to lock mutex");
        match update_data(cache.all_processes.clone()) {
            Ok(_) => println!("Cache written!"),
            Err(e) => eprintln!("Error writing cache: {:?}", e)
        }
    }

    pub fn refresh_cache() {
        let data = match read_data() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error reading cache: {:?}", e);
                return; // Instead of exiting, we return to allow further handling.
            }
        };

        let mut cache = Self::get_instance().lock().expect("Failed to lock mutex");
        cache.all_processes = data;
        println!("Cache refreshed!");
    }

    pub fn add_process(process: Process) {
        let mut cache = Self::get_instance().lock().expect("Failed to lock mutex");
        match cache.all_processes.iter().find(|p| p.name == process.name) {
            Some(_) => {
                println!("Process already exists");
                return;
            },
            None => {
                cache.all_processes.push(process);
            }
        }
    }

    pub fn modify_process(process: Process) {
        let mut cache = Self::get_instance().lock().expect("Failed to lock mutex");
        let index = cache.all_processes.iter().position(|p| p.name == process.name);
        match index {
            Some(i) => {
                cache.all_processes[i] = process;
            },
            None => {
                println!("Process not found");
            }
        }
    }

    pub fn get_process(&self, process_name: &str) -> Option<Process> {
        return match self.all_processes.iter().find(|p| p.name == process_name) {
            Some(p) => Some(p.clone()),
            None => None
        };
    }
}

fn read_process(process_name: &str) -> Option<Process> {
    let singleton = MyCache::get_instance().lock().unwrap();
    match singleton.get_process(process_name) {
        Some(p) => Some(p),
        None => None
    }
}

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

