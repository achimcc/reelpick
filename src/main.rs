//! The program around the library: read the configuration, then either pick
//! today's film or serve the pages. Every step fails closed.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use reelpick::config::{Config, read_secret};
use reelpick::jellyfin::JellyfinClient;
use reelpick::ollama::OllamaClient;
use reelpick::pick::{Clients, Outcome};
use reelpick::store::Store;
use reelpick::tmdb::TmdbClient;

const USAGE: &str = "usage: reelpick <pick|serve|--version>";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.first().map(String::as_str) {
        Some("--version") => {
            println!("reelpick {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("pick") => run(pick),
        Some("serve") => run(serve),
        _ => Err(anyhow::anyhow!(USAGE)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("reelpick: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run<F, Fut>(f: F) -> anyhow::Result<()>
where
    F: FnOnce(Config) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    let path = PathBuf::from(
        std::env::var("REELPICK_CONFIG").unwrap_or_else(|_| "/etc/reelpick/reelpick.toml".into()),
    );
    let config = Config::load(&path)?;
    tokio::runtime::Runtime::new()
        .context("starting the runtime")?
        .block_on(f(config))
}

async fn store_for(config: &Config) -> anyhow::Result<Store> {
    tokio::fs::create_dir_all(&config.data_dir)
        .await
        .with_context(|| format!("creating {}", config.data_dir.display()))?;
    Store::open(&config.data_dir.join("reelpick.db")).await
}

async fn pick(config: Config) -> anyhow::Result<()> {
    let key_file = config
        .jellyfin
        .api_key_file
        .as_ref()
        .context("jellyfin.api_key_file is not set; pick cannot read the library without it")?;
    let jellyfin = JellyfinClient::new(&config.jellyfin.url, &read_secret(key_file)?)?;
    let tmdb = match &config.tmdb.api_key_file {
        Some(f) => Some(TmdbClient::new(&read_secret(f)?, &config.language)?),
        None => {
            eprintln!("reelpick: no tmdb.api_key_file; ratings come from Jellyfin");
            None
        }
    };
    let ollama = OllamaClient::new(&config.ollama.url, &config.ollama.model)?;
    let store = store_for(&config).await?;
    let today = config.today().to_string();
    let clients = Clients {
        jellyfin: &jellyfin,
        tmdb: tmdb.as_ref(),
        ollama: &ollama,
    };
    match reelpick::pick::run(&config, &store, clients, &today, &mut rand::rng()).await? {
        Outcome::AlreadyPicked(d) => eprintln!("reelpick: {d} already has its pick"),
        Outcome::Picked(p) => eprintln!("reelpick: {}: {} ({})", p.date, p.title, p.generated_by),
    }
    Ok(())
}

async fn serve(config: Config) -> anyhow::Result<()> {
    let store = store_for(&config).await?;
    let listener = tokio::net::TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("listening on {}", config.listen))?;
    eprintln!(
        "reelpick: listening on {}{}",
        config.listen, config.base_path
    );
    let app = reelpick::web::router(reelpick::web::State { config, store });
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("serving")
}
