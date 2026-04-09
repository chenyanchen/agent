mod app;
mod config;
mod input;
mod ui;

use clap::Parser;

#[derive(Parser)]
#[command(name = "agent", about = "A general-purpose LLM agent")]
struct Cli {
    /// Override the model ID (e.g. gpt-4o, gpt-4-turbo)
    #[arg(short, long)]
    model: Option<String>,
    /// Override the OpenAI-compatible API base URL
    #[arg(long)]
    api_base: Option<String>,
    /// Run in auto mode (skip all confirmations)
    #[arg(long)]
    auto: bool,
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    let mut cfg = config::Config::load();

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

    app::App::run(cfg)
}
