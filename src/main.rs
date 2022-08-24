use clap::Parser;
use std::path::PathBuf;
use std::io::{Read, Write, stdin, stdout};
use std::fs::File;

use json_key_remover::KeyRemover;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Input file path [default: stdin]
    #[clap(short, long, value_parser, value_name = "FILE")]
    input: Option<PathBuf>,

    /// Output file path [default: stdout]
    #[clap(short, long, value_parser, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Comma separated key list to remove
    #[clap(short, long, value_parser, value_name = "KEY1,KEY2,...,KEYn")]
    keys: String,
 
    /// Buffer size in byte.
    #[clap(short, long, value_parser, default_value_t = 1048576)]
    size: u32,
}

fn main() {
    let args = Args::parse();

    // (1) Init
    //  (1) Get input and output stream
    let reader: Box<dyn Read> = if let Some(path_buf) = &args.input {
        Box::new(File::open(path_buf).unwrap())
    } else {
        let stdin = stdin();
        Box::new(stdin)
    };
    let writer: Box<dyn Write> = if let Some(path_buf) = &args.output {
        Box::new(File::create(path_buf).unwrap())
    } else {
        let stdout = stdout();
        Box::new(stdout)
    };
    //  (2) Get vector of keys to remove
    let keys_to_remove: Vec<String> = args.keys.split(',').map(|x| x.to_string()).collect();
    eprintln!("To remove");
    for (idx, key) in keys_to_remove.iter().enumerate() {
        eprintln!(" {}: {}", idx+1, key);
    }
    //  (3) Get buffer size
    let buffer_size = args.size as usize;

    // (2) Init key remover
    let mut key_remover = KeyRemover::init(buffer_size, keys_to_remove);

    // (3) Run
    key_remover.process(reader, writer);
}
