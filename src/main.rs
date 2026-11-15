use std::process::ExitCode;

fn main() -> ExitCode {
    let config_path = match quilla::utils::config::parse_config() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Configuration error: {e}");
            return ExitCode::FAILURE;
        }
    };
    match quilla::utils::operations::run(config_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}
