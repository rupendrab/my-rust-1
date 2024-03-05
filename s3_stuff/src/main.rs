use aws_config::imds::client;
use rusoto_core::{Region, RusotoError, ByteStream};
use rusoto_s3::{HeadObjectRequest, GetObjectRequest, S3Client, S3};
// use rusoto_s3::StreamingBody;
use std::fs::File;
// use tokio::fs::File as AsyncFile;
use std::io::copy;
// use std::io::Write;
use std::error::Error;
use tokio::runtime::Runtime;
// use tokio_util::io::ReaderStream;
// use tokio::io::{AsyncReadExt, BufReader};

mod stream_util;
use stream_util::file_to_bytestream;
mod sdk_util;

// use tokio::io::AsyncReadExt;
// use tokio::io::AsyncWriteExt;

async fn get_s3_object_etag(bucket: &str, key: &str) -> Result<String, RusotoError<rusoto_s3::HeadObjectError>> {
    let region = Region::default();
    let client = S3Client::new(region);

    let req = HeadObjectRequest {
        bucket: bucket.to_string(),
        key: key.to_string(),
        ..Default::default()
    };

    let result = client.head_object(req).await?;

    result.e_tag.ok_or_else(|| RusotoError::Service(rusoto_s3::HeadObjectError::NoSuchKey(key.to_string())))
}

async fn get_s3_file_etag(s3_file_name: &str) -> Result<String, RusotoError<rusoto_s3::HeadObjectError>> {
    match parse_s3_filename(&s3_file_name) {
        Some((bucket, key, _)) => get_s3_object_etag(&bucket, &key).await,
        None => Err(RusotoError::Service(rusoto_s3::HeadObjectError::NoSuchKey(s3_file_name.to_string())))
    }
}

fn download_s3_file(bucket: &str, key: &str, output: &str) -> Result<String, Box<dyn Error>> {
    let region = Region::default();
    let client = S3Client::new(region);
    let rt = Runtime::new()?;

    let get_req = GetObjectRequest {
        bucket: bucket.to_string(),
        key: key.to_string(),
        ..Default::default()
    };

    let result = rt.block_on(client.get_object(get_req))?;

    let mut stream = result.body.unwrap().into_blocking_read();
    let etag = result.e_tag.unwrap();

    let mut file = File::create(output)?;
    copy(&mut stream, &mut file)?;

    Ok(etag)
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

#[test]
fn test_parse_s3_filename() {
    assert_eq!(
        parse_s3_filename("s3://mybucket/myfile.txt"),
        Some(("mybucket".to_string(), "myfile.txt".to_string(), "myfile.txt".to_string()))
    );
    assert_eq!(
        parse_s3_filename("s3://mybucket/myfolder/myfile.txt"),
        Some(("mybucket".to_string(), "myfolder/myfile.txt".to_string(), "myfile.txt".to_string()))
    );
    assert_eq!(parse_s3_filename("myfile.txt"), None);
}

use std::env;
use std::path::PathBuf;

fn download_s3_file_to_temp_dir(s3_file_name: &str)-> Result<String, String> {
    let temp_dir = env::var("tmpdir").expect("tmpdir not set");
    match parse_s3_filename(&s3_file_name) {
        Some((bucket, key, file_name)) => {
            let mut load_path = PathBuf::from(temp_dir);
            load_path.push(file_name);
            let download_file_name = load_path.to_str().ok_or_else(|| "Invalid file path".to_string())?;
            download_s3_file(&bucket, &key, &download_file_name)
                .map_err(|e| format!("Failed to download file: {}", e))
        }
        None => Err(format!("Invalid S3 filename: {}", s3_file_name)),
    }
}

use std::io::Read;

fn upload_local_file_to_s3(local_file: &str, s3_file_name: &str) -> Result<String, Box<dyn Error>> {
    let region = Region::default();
    let client = S3Client::new(region);
    let rt = Runtime::new()?;

    let mut file = File::open(local_file)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    match parse_s3_filename(&s3_file_name) {
        Some((bucket, key, _)) => {
            let put_req = rusoto_s3::PutObjectRequest {
                bucket: bucket.to_string(),
                key: key.to_string(),
                body: Some(buffer.into()),
                ..Default::default()
            };

            let result = rt.block_on(client.put_object(put_req))?;
            Ok(result.e_tag.unwrap())
        }
        None => Err("Invalid S3 filename".into()),
    }

}

use futures::{Future, StreamExt};

/*
fn print_byte_stream(mut byte_stream: ByteStream) -> Result<(), Box<dyn Error>> {
    // Create a new runtime
    let rt = Runtime::new()?;

    // Block on the async function
    rt.block_on(async {
        while let Some(item) = byte_stream.next().await {
            match item {
                Ok(bytes) => {
                    println!("{:?}", bytes); // print the bytes
                }
                Err(e) => {
                    eprintln!("Error: {:?}", e); // print the error
                }
            }
        }
    });

    Ok(())
}
*/

/// Says this functionality is not implemented
fn upload_local_file_to_s3_lomem(bucket: &str, key: &str, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let region = Region::default();
    let client = S3Client::new(region);
    let rt = Runtime::new()?;

    let stream = rt.block_on(file_to_bytestream(file_path));
    // print_byte_stream(stream)?;

    let req = rusoto_s3::PutObjectRequest {
        bucket: bucket.to_string(),
        key: key.to_string(),
        body: Some(stream),
        ..Default::default()
    };

    rt.block_on(client.put_object(req))?;

    Ok(())
}

use aws_sdk_s3::Client;
use sdk_util::{upload_object, get_client};
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
use aws_config::meta::region::RegionProviderChain;
use tokio::runtime::Handle;
use std::sync::Arc;
use once_cell::sync::OnceCell;

static S3_CLIENT: OnceCell<Arc<Client>> = OnceCell::new();

pub fn get_client_new() -> Arc<Client> {
    S3_CLIENT.get().expect("S3_CLIENT not initialized").clone()
}

async fn upload_local_file_to_s3_sdk(local_file: &str, s3_file_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    // let client = get_client().await?;
    let client = get_client_new();
    match parse_s3_filename(&s3_file_name) {
        Some((bucket, key, _)) => {
            upload_object(&client, &bucket, &local_file, &key).await
        },
        None => Err("Invalid S3 filename".into())
    }
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

use std::time::Instant;

fn main() {
    let client = sdk_util::init_s3_client();
    S3_CLIENT.set(client).expect("Failed to set S3_CLIENT");
    println!("Set up new S3 Client!");
    let s3_file_name = "s3://i360-dev-political-dmi/DMI_ABEV/DMI_ABEV_VA_20231113095455_split0000001.csv.gz";
    
    match download_s3_file_to_temp_dir(&s3_file_name) {
        Ok(etag) => println!("Successfully downloaded file with etag: {}", etag),
        Err(e) => eprintln!("Error: {}", e)
    };

    let start = Instant::now();

    let rt = tokio::runtime::Runtime::new().unwrap();
    for _ in 0..1 {
        match rt.block_on(get_s3_file_etag(&s3_file_name)) {
            Ok(etag) => println!("Successfully found etag: {}", etag),
            Err(e) => eprintln!("Error: {}", e)
        };
    }

    let duration = start.elapsed();
    println!("Time elapsed in expensive_function() is: {:?}", duration);

    let local_filename = "../cache_test/processes.json";
    let s3_file_json = "s3://i360-dev-political-dmi/API_CONTROL/processes.json";
    match upload_local_file_to_s3(&local_filename, &s3_file_json) {
        Ok(etag) => println!("Successfully uploaded file {} with etag: {}", &s3_file_json, etag),
        Err(e) => eprintln!("Error: {}", e)
    };

    if 1 == 0 {
        match upload_local_file_to_s3_lomem("i360-dev-political-dmi", "API_CONTROL/processes.json", &local_filename) {
            Ok(_) => println!("Successfully uploaded file to S3 using the new process"),
            Err(e) => eprintln!("Error: {}", e)
        };
    }

    match rt.block_on(upload_local_file_to_s3_sdk(&local_filename, &s3_file_json)) {
        Ok(etag) => println!("Successfully uploaded file {} with etag: {}", &s3_file_json, etag),
        Err(e) => eprintln!("Error: {}", e)
    };

    // Get e-tags using the SDK
    let start = Instant::now();

    // let client = rt.block_on(get_client()).unwrap();
    let counter = 20;
    for _ in 0..counter {
        match rt.block_on(get_etag(&s3_file_json)) {
            Ok(etag) => println!("Successfully found etag: {}", etag),
            Err(e) => eprintln!("Error: {}", e)
        };
    }
    let duration = start.elapsed();
    let avg_duration = (duration.as_millis() as f64 / counter as f64);
    println!(
        "Time elapsed in retrieving {} e-tags is: {:?} milliseconds , average = {} milliseconds", 
        counter, duration.as_millis(), 
        avg_duration);

    // Download files using the SDK
    match rt.block_on(dowload_file_from_s3_to_temp_dir_sdk(&s3_file_json)) {
        Ok(etag) => println!("Successfully downloaded file with etag: {}", etag),
        Err(e) => eprintln!("Error: {}", e)
    };

}
