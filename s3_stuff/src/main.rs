use rusoto_core::Region;
use rusoto_s3::{GetObjectRequest, S3Client, S3};
use std::fs::File;
use std::io::Write;
use std::error::Error;
use tokio::runtime::Runtime;
use tokio::io::AsyncReadExt;

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

fn main() {
    let s3_file_name = "s3://i360-prod-political-dmi/DMI_ABEV/DMI_ABEV_WV_20221118093441_split0000001.csv.gz";
    match download_s3_file_to_temp_dir(&s3_file_name) {
        Ok(etag) => println!("Successfully downloaded file with etag: {}", etag),
        Err(e) => eprintln!("Error: {}", e)
    };
}
