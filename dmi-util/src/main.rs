use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use clap::Parser;

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

fn main() {
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
