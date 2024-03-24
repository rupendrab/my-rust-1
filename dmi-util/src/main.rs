use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use clap::{Parser, Arg, Command};
use aws_sdk_secretsmanager as secretsmanager;
use serde_json::{Map, Value, Result as ResultJson};
use tokio_postgres::{Row, NoTls, Error};
use std::fmt::{self, Display, Formatter};
use chrono::{format, DateTime, NaiveDateTime, Utc};
// Import the Json wrapper type
use postgres_types::Json;
mod db_util_dbdesttable;
mod db_util_dbdest;
mod db_util_secretsmap;

/// A program that replaces all occurrences of a string with another string in a file.
/// A special usage is to replace "-@n" with "-{nver}".
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The input file to read from
    #[arg(short, long)]
    input_file_name: String,

    /// The output file to write to
    #[arg(short, long)]
    output_file_name: String,
    
    /// The version to replace "-@n" with
    #[arg(short, long, required = false)]
    nver: Option<String>,
    
    /// The string to find and replace
    #[arg(short, long, required = false)]
    find_string: Option<String>,
    
    /// The string to replace with
    #[arg(short, long, required = false)]
    replace_string: Option<String>,
}

enum MyError {
    IoError(std::io::Error),
    PostgresError(tokio_postgres::Error),
    SecretsManagerError(secretsmanager::Error),
    SerdeJsonError(serde_json::Error),
    AnyHowError(anyhow::Error),
    // Add other error types as needed.
}

impl From<std::io::Error> for MyError {
    fn from(error: std::io::Error) -> Self {
        MyError::IoError(error)
    }
}

impl From<tokio_postgres::Error> for MyError {
    fn from(error: tokio_postgres::Error) -> Self {
        MyError::PostgresError(error)
    }
}

impl From<secretsmanager::Error> for MyError {
    fn from(error: secretsmanager::Error) -> Self {
        MyError::SecretsManagerError(error)
    }
}

impl From<serde_json::Error> for MyError {
    fn from(error: serde_json::Error) -> Self {
        MyError::SerdeJsonError(error)
    }
}

impl From<anyhow::Error> for MyError {
    fn from(error: anyhow::Error) -> Self {
        MyError::AnyHowError(error)
    }
}

impl Display for MyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            MyError::IoError(err) => write!(f, "IO error: {}", err),
            MyError::PostgresError(err) => write!(f, "Postgres error: {}", err),
            MyError::SecretsManagerError(err) => write!(f, "Secrets Manager error: {}", err),
            MyError::SerdeJsonError(err) => write!(f, "Serde JSON error: {}", err),
            MyError::AnyHowError(err) => write!(f, "AnyHow error: {}", err),
            // Handle other cases as needed.
        }
    }
}

fn replace_file(input_file_name: &str, output_file_name: &str, find_string: &str, replace_string: &str) -> io::Result<()> {
    let input_path = Path::new(input_file_name);
    let output_path = Path::new(output_file_name);

    // Open the input file for reading
    let input_file = File::open(input_path)?;
    let reader = BufReader::new(input_file);

    // Create the output file for writing
    let mut output_file = File::create(output_path)?;

    // Iterate over each line in the input file
    for line in reader.lines() {
        let line = line?;
        // Replace all occurrences of "-@n" with `-{nver}`
        let modified_line = line.replace(find_string, replace_string);
        // Write the modified line to the output file, including a newline character
        writeln!(output_file, "{}", modified_line)?;
    }

    Ok(())
}

fn replace_and_write_file(input_file_name: &str, output_file_name: &str, nver: &str) -> io::Result<()> {
    replace_file(input_file_name, output_file_name, "-@n", &format!("-{}", nver))
}

async fn get_secret(secret_name: &str) -> Result<String, secretsmanager::Error> {
    let config = aws_config::load_from_env().await;
    let client = aws_sdk_secretsmanager::Client::new(&config);
    let request = client.get_secret_value().secret_id(secret_name);
    let response = request.send().await.unwrap();
    let secret_value = response.secret_string.unwrap();
    Ok(secret_value)
}

fn extract_from_json(json_str: &str) -> ResultJson<(String, String, String, String)> {
    // Parse the string of data into serde_json::Value.
    let v: Value = serde_json::from_str(json_str)?;

    // Extract values for the specified keys.
    let username = v["username"].as_str().unwrap_or_default().to_string();
    let password = v["password"].as_str().unwrap_or_default().to_string();
    let port = v["port"].as_str().unwrap_or_default().to_string();
    let endpoint = v["endpoint"].as_str().unwrap_or_default().to_string();

    Ok((username, password, port, endpoint))
}

async fn get_connect_string(secret_name: &str, db: &str) -> Result<String, MyError> {
    let secret = get_secret(secret_name).await?;
    let (username, password, port, endpoint) = extract_from_json(&secret)?;
    Ok(format!("host={} port={} user={} password={} dbname={}", endpoint, port, username, password, db))
}

async fn fetch_data_and_convert_to_json(secret_name: &str, db: &str, sql: &str) -> Result<Vec<Value>, MyError> {
    // Connect to the database (adjust the connection string as necessary)
    let connect_str = get_connect_string(secret_name, db).await
        .map_err(|e| e)?;
    
    // println!("Connecting to database with connection string: {}", connect_str);
    let (client, connection) = tokio_postgres::connect(&connect_str, NoTls).await?;

    // The connection object performs the actual communication with the database,
    // so spawn it off to run on its own.
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    // Now, we can execute a query and fetch the result
    let rows = client.query(sql, &[]).await?;

    // Convert the rows to JSON values directly
    let mut json_rows = Vec::new();
    for row in rows {
        let mut json_row = serde_json::Map::new();
        for (i, col) in row.columns().iter().enumerate() {
            let key = col.name().to_string();
            println!("Column = {}, Type = {}", key, col.type_().name());
            let value: Value = match col.type_().name() {
                "int4" => serde_json::to_value(row.get::<_, i32>(i)).unwrap_or(Value::Null),
                "text" => serde_json::to_value(row.get::<_, String>(i)).unwrap_or(Value::Null),
                "varchar" => serde_json::to_value(row.get::<_, String>(i)).unwrap_or(Value::Null),
                "timestamp" => serde_json::to_value(row.get::<_, NaiveDateTime>(i)).unwrap_or(Value::Null),
                "timestamptz" => serde_json::to_value(row.get::<_, DateTime<Utc>>(i)).unwrap_or(Value::Null),
                "jsonb" => {
                    let jsonb: Option<Json<serde_json::Value>> = row.get(i);
                    jsonb.map_or(Value::Null, |j| j.0)
                },
                // Add more types as necessary
                _ => Value::Null,
            };
            json_row.insert(key, value);
        }
        json_rows.push(Value::Object(json_row));
    }

    Ok(json_rows)
}

/*
fn old_main() {
    let args = Args::parse();
    match &args.nver {
        Some(nver) => 
            match replace_and_write_file(&args.input_file_name, &args.output_file_name, nver)  {
                Ok(()) => println!("File processing complete."),
                Err(e) => eprintln!("Error processing file: {}", e),
            }
        None => {},
    }
    match (&args.find_string, &args.replace_string) {
        (Some(find_string), Some(replace_string)) => 
            match replace_file(&args.input_file_name, &args.output_file_name, find_string, replace_string) {
                Ok(()) => println!("File processing complete."),
                Err(e) => eprintln!("Error processing file: {}", e),
            }
        _ => {},
    }
}
*/

async fn create_config_records_qn(secret_name: &str, db: &str, qno: &i64)-> Result<(), MyError> {
    // Connect to the database (adjust the connection string as necessary)
    let connect_str = get_connect_string(secret_name, db).await
        .map_err(|e| e)?;
    
    // println!("Connecting to database with connection string: {}", connect_str);
    let (mut client, connection) = tokio_postgres::connect(&connect_str, NoTls).await?;

    // The connection object performs the actual communication with the database,
    // so spawn it off to run on its own.
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let mut success = true;
    let dest_id = format!("pg-queryable-{}", qno);
    let template_file = "queryable_templates/dbdesttable.json";
    match db_util_dbdesttable::insert_records_from_file(&mut client, template_file, &dest_id).await {
        Ok(()) => {
            println!("DB Dest Tables created successfully.");
        },
        Err(e) => {
            eprintln!("Error creating DB Dest Tables: {}", e);
            success = false;
            // Err(MyError::PostgresError(anyhow::Error::new(e)))
            // Err(MyError::AnyHowError(e))
        },
    }

    if ! success {
        return Err(MyError::AnyHowError(anyhow::anyhow!("One or more operations failed, transaction rolled back")));
    }

    let template_file = "queryable_templates/dbdest.json";
    let dest_server = format!("pg-queryable-{}-v2", qno);
    match db_util_dbdest::insert_records_from_file(&mut client, template_file, &dest_id, &dest_server).await {
        Ok(()) => {
            println!("DB Dests created successfully.");
        },
        Err(e) => {
            eprintln!("Error creating DB Dests: {}", e);
            success = false;
            // Err(MyError::PostgresError(anyhow::Error::new(e)))
            // Err(MyError::AnyHowError(e))
        },
    }

    if ! success {
        return Err(MyError::AnyHowError(anyhow::anyhow!("One or more operations failed, transaction rolled back")));
    }

    let template_file = "queryable_templates/secrets_map.json";
    let secret_alias = format!("pg-queryable-{}-v2", qno);
    let secret_namae = format!("apg/prod-i360-portal-query{}/dmi", qno);
    match db_util_secretsmap::insert_records_from_file(&mut client, template_file, &secret_alias, &secret_namae).await {
        Ok(()) => {
            println!("Secrets Maps created successfully.");
        },
        Err(e) => {
            eprintln!("Error creating Secrets Maps: {}", e);
            success = false;
            // Err(MyError::PostgresError(anyhow::Error::new(e)))
            // Err(MyError::AnyHowError(e))
        },
    }

    if ! success {
        return Err(MyError::AnyHowError(anyhow::anyhow!("One or more operations failed, transaction rolled back")));
    }

    Ok(())

}

fn main() {
    let matches = Command::new("dmi-app")
        .version("1.0")
        .author("Rupendra Bandyopadhyay <rbandyopadhyay@i-360.com>")
        .about("Some DMI script utilities")
        .subcommand(
            Command::new("replace")
                .about("Replace all occurrences of a string with another string in a file")
                .arg(
                    Arg::new("input_file_name")
                        .short('i')
                        .long("input")
                        .help("The input file to read from")
                        .required(true)
                )
                .arg(
                    Arg::new("output_file_name")
                        .short('o')
                        .long("output")
                        .help("The output file to write to")
                        .required(true)
                )
                .arg(
                    Arg::new("nver")
                        .short('n')
                        .long("nver")
                        .help("The version to replace '-@n' with")
                )
                .arg(
                    Arg::new("find_string")
                        .short('f')
                        .long("find")
                        .help("The string to find and replace")
                )
                .arg(
                    Arg::new("replace_string")
                        .short('r')
                        .long("replace")
                        .help("The string to replace with")
                )
            )
        .subcommand(
            Command::new("read_secret")
                .about("Read a secret from AWS Secrets Manager")
                .arg(
                    Arg::new("secret_name")
                        .short('s')
                        .long("secret")
                        .help("The name of the secret to read")
                        .required(true)
                )
            )
        .subcommand(
            Command::new("fetch_data")
                .about("Fetch data from a database and convert to JSON")
                .arg(
                    Arg::new("secret_name")
                        .short('s')
                        .long("secret")
                        .help("The name of the secret to read")
                        .required(true)
                )
                .arg(
                    Arg::new("db")
                        .short('d')
                        .long("database")
                        .help("The name of the database to connect to")
                        .required(true)
                )
                .arg(
                    Arg::new("sql")
                        .short('q')
                        .long("query")
                        .help("The SQL query to execute")
                        .required(true)
                )
        )
        .subcommand(
            Command::new("create_db_config")
                .about("Create DB config records for a new postgres instance")
                .arg(
                    Arg::new("secret_name")
                        .short('s')
                        .long("secret")
                        .help("The name of the secret for database connection")
                        .required(true)
                )
                .arg(
                    Arg::new("db")
                        .short('d')
                        .long("database")
                        .help("The name of the database to connect to")
                        .required(true)
                )
                .arg(
                    Arg::new("qno")
                        .short('q')
                        .long("qno")
                        .value_parser(clap::value_parser!(i64))
                        .help("The number of the Postgres queryable instance")
                        .required(true)
                )
            )
        .get_matches();

    match matches.subcommand() {
        Some(("replace", replace_matches)) => {
            let input_file_name = replace_matches.get_one::<String>("input_file_name").unwrap();
            let output_file_name = replace_matches.get_one::<String>("output_file_name").unwrap();
            let nver = replace_matches.get_one::<String>("nver").map(|s| s.as_str()).unwrap_or("");
            let find_string = replace_matches.get_one::<String>("find_string").map(|s| s.as_str()).unwrap_or("");
            let replace_string = replace_matches.get_one::<String>("replace_string").map(|s| s.as_str()).unwrap_or("");

            if !nver.is_empty() {
                match replace_and_write_file(input_file_name, output_file_name, nver) {
                    Ok(()) => println!("File processing complete."),
                    Err(e) => eprintln!("Error processing file: {}", e),
                }
            }

            if !find_string.is_empty() && !replace_string.is_empty() {
                match replace_file(input_file_name, output_file_name, find_string, replace_string) {
                    Ok(()) => println!("File processing complete."),
                    Err(e) => eprintln!("Error processing file: {}", e),
                }
            }
        }
        Some(("read_secret", read_secret_matches)) => {
            let secret_name = read_secret_matches.get_one::<String>("secret_name").unwrap();
            match tokio::runtime::Runtime::new().unwrap().block_on(get_secret(&secret_name)) {
                Ok(secret) => println!("Secret: {}", secret),
                Err(e) => eprintln!("Error reading secret: {}", e),
            }
        }
        Some(("fetch_data", fetch_data_matches)) => {
            let secret_name = fetch_data_matches.get_one::<String>("secret_name").unwrap();
            let db = fetch_data_matches.get_one::<String>("db").unwrap();
            let sql = fetch_data_matches.get_one::<String>("sql").unwrap();
            match tokio::runtime::Runtime::new().unwrap().block_on(fetch_data_and_convert_to_json(&secret_name, &db, &sql)) {
                Ok(json_rows) => {
                    /*
                    for row in json_rows {
                        println!("{}", row);
                    }
                    */
                    serde_json::to_writer_pretty(io::stdout(), &json_rows).unwrap();
                    let no_rows = json_rows.len();
                    println!("\n{} rows fetched.", no_rows);
                }
                Err(e) => eprintln!("Error fetching data: {}", e),
            }
        }
        Some(("create_db_config", create_db_config_matches)) => {
            let secret_name = create_db_config_matches.get_one::<String>("secret_name").unwrap();
            let db = create_db_config_matches.get_one::<String>("db").unwrap();
            let qno = create_db_config_matches.get_one::<i64>("qno").unwrap();
            match tokio::runtime::Runtime::new().unwrap().block_on(create_config_records_qn(&secret_name, &db, qno)) {
                Ok(()) => println!("DB Config records created successfully."),
                Err(e) => eprintln!("Error creating DB Config records: {}", e),
            }
        }
        _ => {}
    }
}
