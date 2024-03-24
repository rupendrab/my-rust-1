use serde_json::Value;
use std::fs;
use tokio_postgres::{Client, NoTls, Error};
use tokio_postgres::Error as PgError;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct DbDest {
    dest_db: String,
    dest_db_type: String,
    dest_id: String,
    dest_server: String,
    dest_type: String,
}

async fn read_and_parse_json(file_path: &str) -> Result<Vec<DbDest>> {
    // Read the file to a string
    let data = fs::read_to_string(file_path)?;

    // Parse the string as JSON into Vec<User>
    let recs: Vec<DbDest> = serde_json::from_str(&data)?;

    Ok(recs)
}

fn modify_records(recirds: Vec<DbDest>, new_db_dest: &str, new_dest_server: &str) -> Vec<DbDest> {
    let mut new_records = Vec::new();
    for mut record in recirds {
        record.dest_id = new_db_dest.to_string();
        record.dest_server = new_dest_server.to_string();
        new_records.push(record);
    }
    new_records
}

async fn insert_records(client: &mut Client, records: Vec<DbDest>) -> Result<()> {
    let transaction = client.transaction().await?;
    let mut success = true;
    for record in records {
        let res = transaction.execute(
            r#"INSERT INTO config.db_dests 
                    (dest_db, dest_db_type, dest_id, dest_server, dest_type)
                VALUES 
                    ($1, $2, $3, $4, $5)
                ON CONFLICT (dest_id) DO NOTHING"#,
            &[
                &record.dest_db,
                &record.dest_db_type,
                &record.dest_id,
                &record.dest_server,
                &record.dest_type,
            ]
        ).await;
        if res.is_err() {
            eprintln!("Error inserting record: {:?}", res.err());
            success = false;
            break;
        }
    }
    
    if success {
        // If all operations were successful, commit the transaction
        if let Err(e) = transaction.commit().await {
            eprintln!("Failed to commit transaction: {}", e);
            // Handle commit error (optional)
            return Err(Box::new(e).into());
        }
    } else {
        // If any operation failed, roll back the transaction
        if let Err(e) = transaction.rollback().await {
            eprintln!("Failed to roll back transaction: {}", e);
            // Handle rollback error (optional)
            return Err(Box::new(e).into());
        }
        // Return or handle the error appropriately
        return Err(anyhow::anyhow!("One or more operations failed, transaction rolled back"));
    }

    Ok(())
}

pub async fn insert_records_from_file(client: &mut Client, file_path: &str, new_db_dest: &str, new_dest_server: &str) -> Result<()> {
    let records = read_and_parse_json(file_path).await?;
    let records = modify_records(records, new_db_dest, new_dest_server);
    insert_records(client, records).await
}


