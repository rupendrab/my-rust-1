use std::collections::HashMap;
use std::hash::Hash;
use aws_sdk_s3::operation::put_object;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use std::fs::{soft_link, File};
use std::io::{BufReader};
use serde::{Deserialize, Serialize};
use serde_json::{from_reader};
use anyhow::Result;

use aws_sdk_s3::Client;
use std::sync::Arc;
use once_cell::sync::OnceCell;
use std::env;
use std::path::PathBuf;
use tokio::runtime::Runtime;
use tokio::task::spawn_blocking;
use log::{debug, error, log_enabled, info, Level};
use chrono::Local;
use chrono::format::strftime::StrftimeItems;

use crate::ProcessQueryParams;

pub static S3_CLIENT: OnceCell<Arc<Client>> = OnceCell::new();

pub fn get_client_new() -> Arc<Client> {
    S3_CLIENT.get().expect("S3_CLIENT not initialized").clone()
}

fn parse_s3_filename(filename: &str) -> Option<(String, String, String)> {
    let s3_prefix = "s3://";
    if !filename.starts_with(s3_prefix) {
        return None;
    }

    let without_prefix = &filename[s3_prefix.len()..];
    let parts: Vec<&str> = without_prefix.splitn(2, '/').collect();
    if parts.len() != 2 {
        return None;
    }

    let bucket = parts[0].to_string();
    let key = parts[1].to_string();

    let key_parts: Vec<&str> = key.rsplitn(2, '/').collect();
    let file_name = key_parts[0].to_string();

    Some((bucket, key, file_name))
}

async fn get_etag(s3_file_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = get_client_new();
    match parse_s3_filename(&s3_file_name) {
        Some((bucket, key, _)) => {
            let head_object_output = client.head_object().bucket(&bucket).key(&key).send().await?;
            match head_object_output.e_tag {
                Some(etag) => Ok(etag),
                None => Err("Failed to get ETag from HeadObjectOutput".into())
            }
        },
        None => Err("Invalid S3 filename".into())
    }
}

async fn dowload_file_from_s3_to_temp_dir_sdk(s3_file_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = get_client_new();
    match parse_s3_filename(&s3_file_name) {
        Some((bucket, key, file_name)) => {
            let temp_dir = env::var("tmpdir").expect("tmpdir not set");
            let mut load_path = PathBuf::from(temp_dir);
            load_path.push(file_name);
            let download_file_name = load_path.to_str().ok_or_else(|| "Invalid file path".to_string())?;
            let get_object_output = client.get_object().bucket(&bucket).key(&key).send().await?;
            let mut stream = get_object_output.body.into_async_read();
            let mut file = tokio::fs::File::create(download_file_name).await?;
            tokio::io::copy(&mut stream, &mut file).await?;
            Ok(get_object_output.e_tag.unwrap())
        },
        None => Err("Invalid S3 filename".into())
    }
}

async fn upload_file_to_s3_(s3_file_name: &str, file_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = get_client_new();
    match parse_s3_filename(&s3_file_name) {
        Some((bucket, key, _)) => {
            match upload_object(&client, &bucket, file_path, &key).await {
                Ok(output) => Ok(output),
                Err(e) => Err(format!("Error uploading file: {:?}", e).into())
            }
        },
        None => Err("Invalid S3 filename".into())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Process {
  pub name: String,
  pub run: bool,
  pub tags: Option<Vec<String>>,
  pub effective: String, // Store effective date/time as a string for simplicity
}

pub struct MyCache {
    pub all_processes: HashMap<String, Process>,
    pub cache_time: u64,
    pub etag: String,
}

pub fn create_process(name: &str, run: bool, tags: Option<Vec<String>>) -> Process {
    let now = Local::now();
    let effective = now.format("%Y-%m-%d %H:%M:%S").to_string();
    Process {
        name: name.to_string(),
        run,
        tags,
        effective,
    }
}

pub fn update_process_partial(process: &mut Process, run: Option<bool>, tags: Option<Vec<String>>) {
    if let Some(t) = tags {
        process.tags = Some(t); // Update tags only if Some(tags) is provided
    }
    if let Some(r) = run {
        process.run = r;
    }
    let now = Local::now();
    process.effective = now.format("%Y-%m-%d %H:%M:%S").to_string();
}

fn update_process(process: &mut Process, run: bool, tags: Option<Vec<String>>) {
    process.run = run;
    if let Some(t) = tags {
        process.tags = Some(t); // Update tags only if Some(tags) is provided
    }
    let now = chrono::Local::now();
    process.effective = now.format("%Y-%m-%d %H:%M:%S").to_string();
}

fn to_map(processes: Vec<Process>) -> HashMap<String, Process> {
    let mut map = HashMap::new();
    for p in processes {
        map.insert(p.name.clone(), p);
    }
    map
}

fn to_list(processes: &HashMap<String, Process>) -> Vec<Process> {
    let mut list = Vec::new();
    for (_, v) in processes {
        list.push(v.clone());
    }
    list
}

pub async fn read_data() -> Result<(HashMap<String, Process>, String), Box<dyn std::error::Error>> {
    let s3_file_name = env::var("s3_file").expect("s3_file not set");
    let temp_dir = env::var("tmpdir").expect("tmpdir not set");
    let mut load_path = PathBuf::from(temp_dir);
    match parse_s3_filename(&s3_file_name) {
        Some((_, _, file_name)) => {
            load_path.push(file_name);
            match dowload_file_from_s3_to_temp_dir_sdk(s3_file_name.as_str()).await {
                Ok(etag) => {
                    info!("Downloaded s3 file with ETag: {}", etag);
                    let file = File::open(load_path)?;
                    let reader = BufReader::new(file);
                  
                    // Read the JSON data from the file
                    let data: Vec<Process> = from_reader(reader)?;
                    let data_map = to_map(data);
                    Ok((data_map, etag))
                },
                Err(e) => {
                    Err(format!("Error downloading file: {:?}", e).into())
                }
            }
        },
        None => Err("Invalid S3 filename".into())
    }
}

pub fn get_current_time() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

async fn update_data(data: HashMap<String, Process>, etag: &str) -> Result<String, Box<dyn std::error::Error>> {
    let s3_file_name = env::var("s3_file").expect("s3_file not set");
    let temp_dir = env::var("tmpdir").expect("tmpdir not set");
    let mut load_path = PathBuf::from(temp_dir);
    match parse_s3_filename(&s3_file_name) {
        Some((_, _, file_name)) => {
            load_path.push(file_name);
            let upload_file_name = load_path.to_str().ok_or_else(|| "Invalid file path".to_string())?;
            let file = File::create(&upload_file_name)?;
            let data_list = to_list(&data);
            serde_json::to_writer(file, &data_list)?;
            let etag_new = get_etag(s3_file_name.as_str()).await?;
            if etag != etag_new {
                return Err("Please retry the operation".into());
            }
            match upload_file_to_s3_(&s3_file_name, upload_file_name).await {
                Ok(etag_c) => Ok(etag_c),
                Err(e) => Err(format!("Error uploading file: {:?}", e).into())
            }
        },
        None => return Err("Invalid S3 filename".into())
    }
}

async fn init_cache() -> MyCache {
    match read_data().await {
        Ok((data, etag)) => MyCache {
            all_processes: data,
            cache_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            etag,
        },
        Err(e) => {
            error!("Error reading cache: {:?}", e);
            std::process::exit(1);
        }
    }
}

fn filter_processes_by_tags(processes: Vec<Process>, input_tags: &Option<Vec<String>>) -> Vec<Process> {
    // print input_tags
    match input_tags {
        Some(tags) => processes.into_iter()
            .filter(|process| {
                // If process.tags is None, it cannot contain any tags, so exclude it by returning false
                if let Some(process_tags) = &process.tags {
                    // Check if all input tags are contained within the process's tags
                    tags.iter().all(|tag| process_tags.contains(tag))
                } else {
                    false
                }
            })
            .collect(),
        None => processes,
    }
}

fn filter_processes_by_name_prefix(processes: Vec<Process>, name_prefixes: &Option<Vec<String>>) -> Vec<Process> {
    match name_prefixes {
        Some(prefixes) => processes.into_iter()
            .filter(|process| {
                // Check if process's name starts with any of the prefixes
                prefixes.iter().any(|prefix| process.name.starts_with(prefix))
            })
            .collect(),
        None => processes,
    }
}

fn filter_processes_by_run(processes: Vec<Process>, run: &Option<bool>) -> Vec<Process> {
    match run {
        Some(r) => processes.into_iter()
            .filter(|process| process.run == *r)
            .collect(),
        None => processes,
    }
}

fn filter_processes(processes: Vec<Process>, query: &ProcessQueryParams) -> Vec<Process> {
    let mut filtered_processes = filter_processes_by_tags(processes, &query.tags);
    filtered_processes = filter_processes_by_name_prefix(filtered_processes, &query.name_prefixes);
    filtered_processes = filter_processes_by_run(filtered_processes, &query.run);
    filtered_processes
}

use tokio::task::block_in_place;

use crate::s3_util::upload_object;

impl MyCache {
    pub async fn get_instance() -> &'static Lazy<Mutex<MyCache>> {
        static INSTANCE: Lazy<Mutex<MyCache>> = Lazy::new(|| {
            let cache = block_in_place(|| tokio::runtime::Runtime::new().unwrap().block_on(init_cache()));
            Mutex::new(cache)
        });
        &INSTANCE
    }

    pub async fn write_cache(&mut self) {
        match update_data(self.all_processes.clone(), &self.etag).await {
            Ok(etag) => {
                info!("Cache written!, new etag = {}", etag);
                self.etag = etag;
            },
            Err(e) => error!("Error writing cache: {:?}", e)
        }
    }

    pub async fn should_refresh_cache(&self) -> bool {
        let time_to_check =  get_current_time()  - self.cache_time > 60;
        if ! time_to_check {
            return false;
        }
        let s3_file_name = env::var("s3_file").expect("s3_file not set");
        match get_etag(&s3_file_name).await {
            Ok(etag_new) => {
                return etag_new != self.etag;
            },
            Err(e) => {
                error!("Error getting ETag: {:?}", e);
                return true; // Decide to refresh cache if we can't get the ETag
            }
        }
    }

    pub async fn refresh_cache(&mut self, force_refresh: bool) {
        if force_refresh || self.should_refresh_cache().await {
            let (processes, etag) = match read_data().await {
                Ok((v, etag)) => (v, etag),
                Err(e) => {
                    error!("Error reading cache: {:?}", e);
                    return; // Instead of exiting, we return to allow further handling.
                }
            };
            self.all_processes = processes;
            self.cache_time = get_current_time();
            self.etag = etag;
            info!("Cache refreshed!");
        }
    }

    pub async fn add_process(&mut self, process: Process) -> Result<(), Box<dyn std::error::Error>> {
        self.refresh_cache(true).await;
        match self.all_processes.entry(process.name.clone()) {
            std::collections::hash_map::Entry::Vacant(e) => {
                // The key does not exist, insert the new process
                e.insert(process);
                self.write_cache().await;
                Ok(())
            },
            std::collections::hash_map::Entry::Occupied(_) => {
                // The key already exists, return an error
                let info = format!("Process {} already exists", process.name);
                info!("{}", &info);
                Err(info.into())
            }
        }
    }

    pub async fn modify_process(&mut self, process: Process) -> Result<(), Box<dyn std::error::Error>> {
        match self.all_processes.entry(process.name.clone()) {
            std::collections::hash_map::Entry::Vacant(_) => {
                // The key does not exist, insert the new process
                let info = format!("Process with name {} does not exist", process.name.clone());
                error!("{}", &info);
                Err(info.into())
            },
            std::collections::hash_map::Entry::Occupied(mut e) => {
                // The key already exists, return an error
                e.insert(process);
                self.write_cache().await;
                Ok(())
            }
        }
    }

    pub async fn delete_process(&mut self, process_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.refresh_cache(true).await;
        match self.all_processes.remove(process_name) {
            Some(_) => {
                self.write_cache().await;
                Ok(())
            },
            None => {
                let info = format!("Process with name {} does not exist", process_name);
                error!("{}", &info);
                Err(info.into())
            }
        }
    }

    pub async fn update_process_partial(&mut self, process_name: &str, run: Option<bool>, tags: Option<Vec<String>>) -> Result<(), Box<dyn std::error::Error>> {
        self.refresh_cache(true).await;
        match self.all_processes.get_mut(process_name) {
            Some(p) => {
                update_process_partial(p, run, tags);
                self.write_cache().await;
                Ok(())
            },
            None => {
                let info = format!("Process with name {} does not exist", process_name);
                error!("{}", &info);
                Err(info.into())
            }
        }
    }

    pub fn get_process(&self, process_name: &str) -> Option<Process> {
        self.all_processes.get(process_name).map(|p| p.clone())
    }

    pub fn filter_processes(&self, query: &ProcessQueryParams) -> Vec<Process> {
        let processes = to_list(&self.all_processes);
        filter_processes(processes, query)
    }
}

pub async fn read_process(process_name: &str) -> Option<Process> {
    let singleton = MyCache::get_instance().await.lock().unwrap();
    match singleton.get_process(process_name) {
        Some(p) => Some(p),
        None => None
    }
}
