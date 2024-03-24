use serde_json::Value;
use std::fs;
use tokio_postgres::{Client, NoTls, Error};
use tokio_postgres::Error as PgError;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct DbDestTable {
    batch_size: i32,
    delete_table_name: String,
    dest_id: String,
    eventtimestamp_column: String,
    pk_columns: Value, // Using Value to directly handle JSON object
    table_name: String,
    table_schema: String,
    table_type: String,
}

async fn read_and_parse_json(file_path: &str) -> Result<Vec<DbDestTable>> {
    // Read the file to a string
    let data = fs::read_to_string(file_path)?;

    // Parse the string as JSON into Vec<User>
    let users: Vec<DbDestTable> = serde_json::from_str(&data)?;

    Ok(users)
}

fn modify_records(recirds: Vec<DbDestTable>, new_db_dest: &str) -> Vec<DbDestTable> {
    let mut new_records = Vec::new();
    for mut record in recirds {
        record.dest_id = new_db_dest.to_string();
        new_records.push(record);
    }
    new_records
}

async fn insert_records(client: &mut Client, records: Vec<DbDestTable>) -> Result<()> {
    let transaction = client.transaction().await?;
    let mut success = true;
    for record in records {
        let jsonb_value = serde_json::to_value(&record.pk_columns)
                                    .context("Failed to serialize pk_columns to JSON")?;
        let res = transaction.execute(
            r#"INSERT INTO config.db_dest_tables 
                    (batch_size, delete_table_name, dest_id, eventtimestamp_column, 
                    pk_columns, table_name, table_schema, table_type) 
                VALUES 
                    ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (dest_id, table_type) DO NOTHING"#,
            &[
                &record.batch_size,
                &record.delete_table_name,
                &record.dest_id,
                &record.eventtimestamp_column,
                &jsonb_value, // Convert to jsonb
                &record.table_name,
                &record.table_schema,
                &record.table_type,
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

pub async fn insert_records_from_file(client: &mut Client, file_path: &str, new_db_dest: &str) -> Result<()> {
    let records = read_and_parse_json(file_path).await?;
    let records = modify_records(records, new_db_dest);
    insert_records(client, records).await
}

