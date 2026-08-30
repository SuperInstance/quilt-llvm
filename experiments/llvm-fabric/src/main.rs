//! llvm-fabric CLI. Subcommands land here stage by stage.

use std::env;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "version" => println!("llvm-fabric 0.1.0 (spike)"),
        _ => {
            eprintln!("usage: llvm-fabric <version>");
            exit(2);
        }
    }
}
