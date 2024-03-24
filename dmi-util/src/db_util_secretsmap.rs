use serde_json::Value;
use std::fs;
use tokio_postgres::{Client, NoTls, Error};
use tokio_postgres::Error as PgError;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct SecretsMap {
    secret_alias: String,
    secret_name: String,
    column_translation: Value,
    extended_metadata: Value,
}

async fn read_and_parse_json(file_path: &str) -> Result<Vec<SecretsMap>> {
    // Read the file to a string
    let data = fs::read_to_string(file_path)?;

    // Parse the string as JSON into Vec<User>
    let recs: Vec<SecretsMap> = serde_json::from_str(&data)?;

    Ok(recs)
}

fn modify_records(records: Vec<SecretsMap>, new_secret_alias: &str, new_secret_name: &str) -> Vec<SecretsMap> {
    let mut new_records = Vec::new();
    for mut record in records {
        record.secret_alias = new_secret_alias.to_string();
        record.secret_name = new_secret_name.to_string();
        new_records.push(record);
    }
    new_records
}

async fn insert_records(client: &mut Client, records: Vec<SecretsMap>) -> Result<()> {
    let transaction = client.transaction().await?;
    let mut success = true;
    for record in records {
        let column_translation = serde_json::to_value(&record.column_translation)
                                    .context("Failed to serialize column_translation to JSON")?;
        let extended_metadata = serde_json::to_value(&record.extended_metadata)
                                    .context("Failed to serialize extended_metadata to JSON")?;
        let res = transaction.execute(
            r#"INSERT INTO config.secrets_map 
                    (secret_alias, secret_name, column_translation, extended_metadata)
                VALUES 
                    ($1, $2, $3, $4)
                ON CONFLICT (secret_alias) DO NOTHING"#,
            &[
                &record.secret_alias,
                &record.secret_name,
                &column_translation,
                &extended_metadata,
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

pub async fn insert_records_from_file(client: &mut Client, file_path: &str, new_secret_alias: &str, new_secret_name: &str) -> Result<()> {
    let records = read_and_parse_json(file_path).await?;
    let records = modify_records(records, new_secret_alias, new_secret_name);
    insert_records(client, records).await
}


