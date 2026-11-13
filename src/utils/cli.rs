use crate::{Error, Result};
use std::{env};

#[derive(Debug)]
pub enum Command {
    Install(Vec<String>),
    Remove(Vec<String>),
    Upgrade,
    Rollback
}

pub fn parse() -> Result<Command> {
    let mut args = env::args().skip(1);
    let sub = args.next().ok_or_else(|| Error::InvalidArgs("No subcommand defined".into()))?;
    let rest: Vec<String> = args.collect();

    match sub.as_str() {
        "install" | "-S" => Ok(Command::Install(rest)),
        "remove" |  "-R" => Ok(Command::Remove(rest)),
        "upgrade" | "-Syu" => Ok(Command::Upgrade),
        "rollback" => Ok(Command::Rollback),
        command => Err(Error::InvalidArgs(format!("Unknown subcommand: {command}")))
    }
}