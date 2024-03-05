use tokio::fs::File;
use tokio::io::BufReader;
use tokio_util::io::ReaderStream;
use bytes::Bytes;
use rusoto_core::ByteStream;
// use futures::stream::{StreamExt, iter};

async fn file_to_stream(path: &str) -> impl futures::Stream<Item = Result<Bytes, std::io::Error>> {
    // Open the file for reading asynchronously
    let file = File::open(path).await.expect("Failed to open file");

    // Create a buffered reader, this is optional but can improve performance
    let reader = BufReader::new(file);

    // Convert the reader into a stream of Bytes
    ReaderStream::new(reader)
}

pub async fn file_to_bytestream(path: &str) -> ByteStream {
    let stream = file_to_stream(path).await;

    ByteStream::new(stream)
}