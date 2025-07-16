use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;
use std::path::PathBuf;
use tokio::signal;
use tracing::{info, error, warn};

mod config;
mod auth;
mod database;
mod messaging;
mod network;
mod utils;

#[cfg(feature = "web")]
mod web;

#[cfg(feature = "terminal")]
mod terminal;

use crate::config::Settings;
use crate::database::Database;
use crate::messaging::MessageSystem;
use crate::network::NetworkManager;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "offgridcomm")]
#[command(about = "Decentralized, low-bandwidth communications platform for ham radio frequencies")]
struct Cli {
    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
    
    /// Data directory path
    #[arg(short, long, value_name = "DIR")]
    data_dir: Option<PathBuf>,
    
    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
    
    /// Run in background/daemon mode
    #[arg(short, long)]
    daemon: bool,
    
    /// Disable RF interface (for testing)
    #[arg(long)]
    no_rf: bool,
    
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the full node (web + terminal + RF)
    Node {
        /// HTTP port for web interface
        #[arg(short, long, default_value = "8080")]
        web_port: u16,
        
        /// Telnet port for terminal interface
        #[arg(short, long, default_value = "2323")]
        terminal_port: u16,
    },
    
    /// Start web interface only
    #[cfg(feature = "web")]
    Web {
        /// HTTP port
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
    
    /// Start terminal interface only
    #[cfg(feature = "terminal")]
    Terminal {
        /// Telnet port
        #[arg(short, long, default_value = "2323")]
        port: u16,
    },
    
    /// Initialize/setup a new node
    Setup {
        /// Node callsign
        #[arg(short, long)]
        callsign: String,
        
        /// Node location (grid square)
        #[arg(short, long)]
        location: Option<String>,
        
        /// Force reinitialize existing node
        #[arg(long)]
        force: bool,
    },
    
    /// Database management commands
    Database {
        #[command(subcommand)]
        action: DatabaseCommands,
    },
    
    /// Import callsign database
    ImportCallsigns {
        /// Path to callsign database file
        file: PathBuf,
        
        /// Database type (fcc, ic, etc.)
        #[arg(short, long, default_value = "fcc")]
        db_type: String,
    },
}

#[derive(Subcommand)]
enum DatabaseCommands {
    /// Initialize database
    Init,
    /// Run database migrations
    Migrate,
    /// Compact/optimize database
    Compact,
    /// Show database statistics
    Stats,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    init_logging(&cli.log_level)?;
    
    info!("Starting OffGridComm v{}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration
    let settings = load_config(cli.config, cli.data_dir)?;
    
    // Initialize database
    let database = Database::new(&settings.database).await?;
    database.migrate().await?;
    
    // Initialize message system
    let message_system = MessageSystem::new(database.clone(), &settings.messaging).await?;
    
    // Initialize network manager
    let network_manager = NetworkManager::new(
        database.clone(),
        message_system.clone(),
        &settings.network,
        !cli.no_rf,
    ).await?;
    
    // Handle different command modes
    match cli.command {
        Some(Commands::Node { web_port, terminal_port }) => {
            run_full_node(
                settings,
                database,
                message_system,
                network_manager,
                web_port,
                terminal_port,
                cli.daemon,
            ).await
        }
        
        #[cfg(feature = "web")]
        Some(Commands::Web { port }) => {
            run_web_only(
                settings,
                database,
                message_system,
                network_manager,
                port,
            ).await
        }
        
        #[cfg(feature = "terminal")]
        Some(Commands::Terminal { port }) => {
            run_terminal_only(
                settings,
                database,
                message_system,
                network_manager,
                port,
            ).await
        }
        
        Some(Commands::Setup { callsign, location, force }) => {
            setup_node(settings, callsign, location, force).await
        }
        
        Some(Commands::Database { action }) => {
            handle_database_command(settings, action).await
        }
        
        Some(Commands::ImportCallsigns { file, db_type }) => {
            import_callsigns(settings, file, db_type).await
        }
        
        None => {
            // Default: run full node
            run_full_node(
                settings,
                database,
                message_system,
                network_manager,
                8080,
                2323,
                cli.daemon,
            ).await
        }
    }
}

async fn run_full_node(
    settings: Settings,
    database: Database,
    message_system: MessageSystem,
    network_manager: NetworkManager,
    web_port: u16,
    terminal_port: u16,
    daemon: bool,
) -> Result<()> {
    info!("Starting full node on web:{} terminal:{}", web_port, terminal_port);
    
    // Start network manager
    let network_handle = tokio::spawn(async move {
        if let Err(e) = network_manager.run().await {
            error!("Network manager error: {}", e);
        }
    });
    
    // Start web interface
    #[cfg(feature = "web")]
    let web_handle = {
        let web_settings = settings.web.clone();
        let web_database = database.clone();
        let web_message_system = message_system.clone();
        
        tokio::spawn(async move {
            if let Err(e) = web::server::run(
                web_settings,
                web_database,
                web_message_system,
                web_port,
            ).await {
                error!("Web server error: {}", e);
            }
        })
    };
    
    // Start terminal interface
    #[cfg(feature = "terminal")]
    let terminal_handle = {
        let terminal_settings = settings.terminal.clone();
        let terminal_database = database.clone();
        let terminal_message_system = message_system.clone();
        
        tokio::spawn(async move {
            if let Err(e) = terminal::server::run(
                terminal_settings,
                terminal_database,
                terminal_message_system,
                terminal_port,
            ).await {
                error!("Terminal server error: {}", e);
            }
        })
    };
    
    // Handle shutdown signals
    let shutdown_signal = async {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
        };
        
        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install signal handler")
                .recv()
                .await;
        };
        
        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();
        
        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    };
    
    info!("OffGridComm node started successfully");
    info!("Web interface: http://localhost:{}", web_port);
    info!("Terminal interface: telnet localhost {}", terminal_port);
    
    if daemon {
        info!("Running in daemon mode");
    } else {
        info!("Press Ctrl+C to shutdown");
    }
    
    // Wait for shutdown signal
    shutdown_signal.await;
    
    info!("Shutting down OffGridComm node...");
    
    // Cancel all tasks
    network_handle.abort();
    
    #[cfg(feature = "web")]
    web_handle.abort();
    
    #[cfg(feature = "terminal")]
    terminal_handle.abort();
    
    info!("OffGridComm node stopped");
    Ok(())
}

#[cfg(feature = "web")]
async fn run_web_only(
    settings: Settings,
    database: Database,
    message_system: MessageSystem,
    network_manager: NetworkManager,
    port: u16,
) -> Result<()> {
    info!("Starting web-only mode on port {}", port);
    
    // Start network manager
    let network_handle = tokio::spawn(async move {
        if let Err(e) = network_manager.run().await {
            error!("Network manager error: {}", e);
        }
    });
    
    // Start web interface
    let web_result = web::server::run(
        settings.web,
        database,
        message_system,
        port,
    ).await;
    
    network_handle.abort();
    web_result
}

#[cfg(feature = "terminal")]
async fn run_terminal_only(
    settings: Settings,
    database: Database,
    message_system: MessageSystem,
    network_manager: NetworkManager,
    port: u16,
) -> Result<()> {
    info!("Starting terminal-only mode on port {}", port);
    
    // Start network manager
    let network_handle = tokio::spawn(async move {
        if let Err(e) = network_manager.run().await {
            error!("Network manager error: {}", e);
        }
    });
    
    // Start terminal interface
    let terminal_result = terminal::server::run(
        settings.terminal,
        database,
        message_system,
        port,
    ).await;
    
    network_handle.abort();
    terminal_result
}

async fn setup_node(
    settings: Settings,
    callsign: String,
    location: Option<String>,
    force: bool,
) -> Result<()> {
    info!("Setting up node with callsign: {}", callsign);
    
    // Validate callsign
    if !auth::callsign::validate(&callsign) {
        return Err(anyhow::anyhow!("Invalid callsign format: {}", callsign));
    }
    
    // Initialize database
    let database = Database::new(&settings.database).await?;
    
    // Check if node already exists
    if !force && database.node_exists().await? {
        return Err(anyhow::anyhow!("Node already initialized. Use --force to reinitialize"));
    }
    
    // Create node configuration
    database.initialize_node(&callsign, location.as_deref()).await?;
    
    info!("Node setup complete!");
    info!("Callsign: {}", callsign);
    if let Some(loc) = location {
        info!("Location: {}", loc);
    }
    
    Ok(())
}

async fn handle_database_command(
    settings: Settings,
    action: DatabaseCommands,
) -> Result<()> {
    let database = Database::new(&settings.database).await?;
    
    match action {
        DatabaseCommands::Init => {
            info!("Initializing database...");
            database.initialize().await?;
            info!("Database initialized successfully");
        }
        
        DatabaseCommands::Migrate => {
            info!("Running database migrations...");
            database.migrate().await?;
            info!("Database migrations completed");
        }
        
        DatabaseCommands::Compact => {
            info!("Compacting database...");
            database.compact().await?;
            info!("Database compaction completed");
        }
        
        DatabaseCommands::Stats => {
            let stats = database.get_stats().await?;
            println!("Database Statistics:");
            println!("  Messages: {}", stats.message_count);
            println!("  Users: {}", stats.user_count);
            println!("  Groups: {}", stats.group_count);
            println!("  Database size: {} MB", stats.size_mb);
        }
    }
    
    Ok(())
}

async fn import_callsigns(
    settings: Settings,
    file: PathBuf,
    db_type: String,
) -> Result<()> {
    info!("Importing callsigns from: {:?}", file);
    
    let database = Database::new(&settings.database).await?;
    
    match db_type.as_str() {
        "fcc" => {
            database.import_fcc_callsigns(&file).await?;
            info!("FCC callsigns imported successfully");
        }
        "ic" => {
            database.import_ic_callsigns(&file).await?;
            info!("Industry Canada callsigns imported successfully");
        }
        _ => {
            return Err(anyhow::anyhow!("Unsupported database type: {}", db_type));
        }
    }
    
    Ok(())
}

fn load_config(config_path: Option<PathBuf>, data_dir: Option<PathBuf>) -> Result<Settings> {
    let mut builder = Config::builder();
    
    // Load default configuration
    builder = builder.add_source(config::File::from_str(
        include_str!("../config/default.toml"),
        config::FileFormat::Toml,
    ));
    
    // Load user configuration if provided
    if let Some(path) = config_path {
        if path.exists() {
            builder = builder.add_source(config::File::from(path));
        } else {
            warn!("Configuration file not found: {:?}", path);
        }
    }
    
    // Override with environment variables
    builder = builder.add_source(config::Environment::with_prefix("OGC"));
    
    let config = builder.build()?;
    let mut settings: Settings = config.try_deserialize()?;
    
    // Override data directory if provided
    if let Some(data_dir) = data_dir {
        settings.data_dir = data_dir;
    }
    
    // Ensure data directory exists
    std::fs::create_dir_all(&settings.data_dir)?;
    
    Ok(settings)
}

fn init_logging(level: &str) -> Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
    
    let log_level = match level {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("offgridcomm={}", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    Ok(())
}