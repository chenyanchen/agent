mod app;
mod config;
mod input;
mod markdown;
mod telemetry;
mod ui;

use std::fs;
use std::io;
use std::path::Path;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "agent", about = "A general-purpose Responses API agent")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Override the model ID (e.g. gpt-4o, gpt-4-turbo)
    #[arg(short, long)]
    model: Option<String>,
    /// Override the OpenAI Responses-compatible API base URL
    #[arg(long)]
    api_base: Option<String>,
    /// Run without interactive tool confirmation
    #[arg(long)]
    auto: bool,
    /// Conversation to create or resume
    #[arg(long, default_value = "default")]
    session: String,
}

#[derive(Subcommand)]
enum Command {
    /// List saved sessions
    Sessions,
    /// Send a test trace to the configured local Langfuse instance
    ObservabilityCheck,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command.as_ref(), Some(Command::Sessions)) {
        for session in session_names(&app::sessions_dir()?)? {
            println!("{session}");
        }
        return Ok(());
    }

    let mut cfg = config::Config::load().map_err(io::Error::other)?;

    // CLI args take precedence over config file.
    if let Some(model_id) = cli.model {
        cfg.model.model_id = model_id;
    }
    if let Some(api_base) = cli.api_base {
        cfg.model.api_base = Some(api_base);
    }
    if cli.auto {
        cfg.guard.mode = config::GuardMode::Auto;
    }

    let telemetry = telemetry::Telemetry::init(&cfg.observability).map_err(io::Error::other)?;
    if matches!(cli.command.as_ref(), Some(Command::ObservabilityCheck)) {
        telemetry::emit_check();
        telemetry.shutdown();
        println!("test trace sent to {}", cfg.observability.endpoint);
        return Ok(());
    }
    let result = app::App::run(cfg, cli.session);
    telemetry.shutdown();
    result
}

fn session_names(dir: &Path) -> io::Result<Vec<String>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
            && let Some(name) = path.file_stem().and_then(|name| name.to_str())
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_json_sessions_in_name_order() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            session_names(&dir.path().join("missing"))
                .unwrap()
                .is_empty()
        );

        fs::write(dir.path().join("zeta.json"), "[]").unwrap();
        fs::write(dir.path().join("alpha.json"), "[]").unwrap();
        fs::write(dir.path().join("ignored.tmp"), "[]").unwrap();

        assert_eq!(session_names(dir.path()).unwrap(), ["alpha", "zeta"]);
    }
}
