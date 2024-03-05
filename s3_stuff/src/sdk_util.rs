
use aws_sdk_s3::operation::{
    copy_object::{CopyObjectError, CopyObjectOutput},
    create_bucket::{CreateBucketError, CreateBucketOutput},
    get_object::{GetObjectError, GetObjectOutput},
    list_objects_v2::ListObjectsV2Output,
    put_object::{PutObjectError, PutObjectOutput},
    head_object::{HeadObjectError, HeadObjectOutput},
};

use aws_sdk_s3::types::{
    BucketLocationConstraint, CreateBucketConfiguration, Delete, ObjectIdentifier,
};

use aws_config::meta::region::RegionProviderChain;

use aws_sdk_s3::{error::SdkError, primitives::ByteStream, Client};
use std::path::Path;
use std::str;

mod error;

pub async fn get_client() -> Result<Client, Box<dyn std::error::Error>> {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env().region(region_provider).load().await;
    let client = Client::new(&config);
    Ok(client)
}

use std::sync::Arc;

pub fn init_s3_client() -> Arc<Client> {
    println!("Initializing S3 client...");
    tokio::runtime::Runtime::new()
        .expect("Failed to create Tokio runtime")
        .block_on(async {
            let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
            let config = aws_config::from_env().region(region_provider).load().await;
            Arc::new(Client::new(&config))
        })
}

pub async fn upload_object(
    client: &Client,
    bucket_name: &str,
    file_name: &str,
    key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let body = ByteStream::from_path(Path::new(file_name)).await;
    let result = client
        .put_object()
        .bucket(bucket_name)
        .key(key)
        .body(body.unwrap())
        .send()
        .await?;

    match result.e_tag {
        Some(etag) => Ok(etag),
        None => Err("Failed to get ETag from PutObjectOutput".into()),
    }
}

