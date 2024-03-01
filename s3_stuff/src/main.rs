use rusoto_core::{Region, RusotoError};
use rusoto_s3::{HeadObjectRequest, GetObjectRequest, S3Client, S3};
use std::fs::File;
use std::io::Write;
use std::error::Error;
use tokio::runtime::Runtime;
use tokio::io::AsyncReadExt;

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

    let stream = result.body.unwrap();
    let etag = result.e_tag.unwrap();

    let mut body = Vec::new();
    rt.block_on(stream.into_async_read().read_to_end(&mut body))?;

    let mut file = File::create(output)?;
    file.write_all(&body)?;

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

use std::time::Instant;

fn main() {
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

}
