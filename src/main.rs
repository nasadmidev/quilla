use quilla::cli::{self, Command};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        },
    }
}

fn run() -> quilla::Result<()> {
    match cli::parse()? {
        Command::Install(pkgs) => println!("Install: {:?}", pkgs),
        Command::Remove(pkgs) => println!("Remove: {:?}", pkgs),
        Command::Upgrade => println!("Update system"),
        Command::Rollback => println!("Return to previous generation")
    };
    Ok(())
}