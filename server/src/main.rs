use std::path::PathBuf;

use clap::{Parser, Subcommand};
use lectern_server::{MIGRATOR, app, auth, build_state, config::Config, jobs, mail, pack};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "lectern", about = "White-label academy server", version)]
struct Cli {
    /// Instance configuration file (TOML).
    #[arg(short, long, env = "LECTERN_CONFIG", global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run migrations, background jobs and the HTTP server (default).
    Serve,
    /// Apply database migrations and exit.
    Migrate,
    /// Import a content pack (directory or .zip).
    ImportPack { path: PathBuf },
    /// Export all content as a pack (.zip).
    ExportPack { output: PathBuf },
    /// Grant a platform role (admin, trainer, channel_manager) to a user, creating the account if needed.
    GrantRole { email: String, role: String },
    /// Check the configuration and print the effective theme.
    CheckConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let json_logs = std::env::var("LECTERN_LOG_FORMAT").is_ok_and(|v| v == "json");
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));
    if json_logs {
        tracing_subscriber::fmt().with_env_filter(filter).json().init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    let cli = Cli::parse();
    let config = Config::load(cli.config.as_deref())?;
    match cli.command.unwrap_or(Command::Serve) {
        Command::CheckConfig => {
            let theme = lectern_server::theme::Theme::from_config(&config);
            println!("configuration OK for {:?}\n{}", config.instance.name, theme.css);
        }
        Command::Migrate => {
            let state = build_state(config).await?;
            MIGRATOR.run(&state.db).await?;
            println!("migrations applied");
        }
        Command::ImportPack { path } => {
            let state = build_state(config).await?;
            MIGRATOR.run(&state.db).await?;
            let files = if path.is_dir() {
                pack::PackFiles::from_dir(&path)?
            } else {
                pack::PackFiles::from_zip(&std::fs::read(&path)?, 2 * 1024 * 1024 * 1024)?
            };
            let report = pack::import(&state.db, &state.config.server.data_dir, &files).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::ExportPack { output } => {
            let state = build_state(config).await?;
            std::fs::write(&output, pack::export(&state.db, &state.config.server.data_dir).await?)?;
            println!("written {}", output.display());
        }
        Command::GrantRole { email, role } => {
            let state = build_state(config).await?;
            MIGRATOR.run(&state.db).await?;
            let role = auth::Role::parse(&role).ok_or_else(|| anyhow::anyhow!("unknown role {role}"))?;
            let id = auth::upsert_user_by_email(&state.db, &email, "").await.map_err(|e| anyhow::anyhow!("{e}"))?;
            sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(id)
                .bind(role.as_str())
                .execute(&state.db)
                .await?;
            println!("granted {} to {email}", role.as_str());
        }
        Command::Serve => {
            let state = build_state(config).await?;
            MIGRATOR.run(&state.db).await?;
            tokio::fs::create_dir_all(state.config.server.data_dir.join("assets")).await?;
            mail::spawn_worker(state.clone());
            jobs::spawn(state.clone());
            let bind = state.config.server.bind.clone();
            let listener = tokio::net::TcpListener::bind(&bind).await?;
            tracing::info!(%bind, instance = %state.config.instance.name, "listening");
            axum::serve(listener, app::router(state)).with_graceful_shutdown(shutdown()).await?;
        }
    }
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
