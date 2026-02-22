//! Conduit Registry -- centralized content discovery index.
//!
//! A thin REST API over SQLite that stores content listings and seeder
//! announcements. Stands in for Nostr relays / DHT during development.
//! See `docs/12_registry.md` for the full specification.
//!
//! Usage:
//!   conduit-registry --port 3003 --db-path /tmp/conduit-registry.db

mod dashboard;
mod db;
mod handlers;
mod types;

use std::sync::{Arc, Mutex};

use axum::routing::{get, post};
use axum::Router;
use clap::Parser;
use rusqlite::Connection;
use tower_http::cors::{Any, CorsLayer};

use crate::db::init_db;
use crate::handlers::{
    create_listing, create_manufacturer, create_seeder, delete_all_listings, delete_all_manufacturers,
    delete_all_seeders, delete_manufacturer, discover, get_listing, get_manufacturer, list_listings,
    list_manufacturers, list_seeders, search_listings,
};
use crate::dashboard::dashboard;
use crate::types::AppState;

#[derive(Parser)]
#[command(name = "conduit-registry")]
#[command(about = "Conduit content discovery registry")]
struct Cli {
    /// HTTP port to listen on
    #[arg(long, default_value = "3003")]
    port: u16,

    /// Path to the SQLite database file
    #[arg(long, default_value = "/tmp/conduit-registry.db")]
    db_path: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Open (or create) SQLite database
    let conn = Connection::open(&cli.db_path).expect("Failed to open database");
    init_db(&conn);
    println!("Database: {}", cli.db_path);

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(dashboard))
        .route("/api/listings", post(create_listing).get(list_listings).delete(delete_all_listings))
        .route("/api/listings/{content_hash}", get(get_listing))
        .route("/api/search", get(search_listings))
        .route("/api/seeders", post(create_seeder).get(list_seeders).delete(delete_all_seeders))
        .route("/api/discover/{content_hash}", get(discover))
        .route("/api/manufacturers", post(create_manufacturer).get(list_manufacturers).delete(delete_all_manufacturers))
        .route("/api/manufacturers/{pk_hex}", get(get_manufacturer).delete(delete_manufacturer))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cli.port);
    println!("Conduit Registry listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
