use crate::{Error, Result};
use std::env;

#[derive(Debug)]
pub enum Command {
    Install(Vec<String>),
    Remove(Vec<String>),
    Upgrade,
    Rollback,
}

pub fn parse() -> Result<Command> {
    parse_args(env::args().skip(1))
}

fn parse_args<I>(args: I) -> Result<Command>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let sub = args
        .next()
        .ok_or_else(|| Error::InvalidArgs("No subcommand defined".into()))?;
    let rest: Vec<String> = args.collect();

    match sub.as_str() {
        "install" | "-S" => Ok(Command::Install(rest)),
        "remove" | "-R" => Ok(Command::Remove(rest)),
        "upgrade" | "-Syu" => Ok(Command::Upgrade),
        "rollback" => Ok(Command::Rollback),
        command => Err(Error::InvalidArgs(format!("Unknown subcommand: {command}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, parse_args};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn parses_commands_and_short_aliases() {
        assert!(
            matches!(parse_args(args(&["install", "bash"])).unwrap(), Command::Install(pkgs) if pkgs == ["bash"])
        );
        assert!(matches!(
            parse_args(args(&["-S", "bash"])).unwrap(),
            Command::Install(_)
        ));
        assert!(matches!(
            parse_args(args(&["-R", "bash"])).unwrap(),
            Command::Remove(_)
        ));
        assert!(matches!(
            parse_args(args(&["-Syu"])).unwrap(),
            Command::Upgrade
        ));
        assert!(matches!(
            parse_args(args(&["rollback"])).unwrap(),
            Command::Rollback
        ));
    }

    #[test]
    fn rejects_missing_and_unknown_commands() {
        assert!(parse_args(Vec::<String>::new()).is_err());
        assert!(parse_args(args(&["unknown"])).is_err());
    }
}
