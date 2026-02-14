//! Conduit Registry -- centralized content discovery index.
//!
//! A thin REST API over SQLite that stores content listings and seeder
//! announcements. Stands in for Nostr relays / DHT during development.
//! See `docs/12_registry.md` for the full specification.
//!
//! Usage:
//!   conduit-registry --port 3003 --db-path /tmp/conduit-registry.db

use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct ContentListing {
    content_hash: String,
    encrypted_hash: String,
    file_name: String,
    size_bytes: u64,
    price_sats: u64,
    chunk_size: u64,
    chunk_count: u64,
    plaintext_root: String,
    encrypted_root: String,
    creator_pubkey: String,
    creator_address: String,
    creator_ln_address: String,
    registered_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SeederAnnouncement {
    encrypted_hash: String,
    seeder_pubkey: String,
    seeder_address: String,
    seeder_ln_address: String,
    transport_price: u64,
    chunk_count: u64,
    announced_at: String,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: Option<String>,
    #[serde(rename = "type")]
    content_type: Option<String>,
    max_price: Option<u64>,
}

#[derive(Debug, Serialize)]
struct DiscoverResponse {
    listing: ContentListing,
    seeders: Vec<SeederAnnouncement>,
}

// ---------------------------------------------------------------------------
// Database initialization
// ---------------------------------------------------------------------------

fn init_db(conn: &Connection) {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS listings (
            content_hash TEXT PRIMARY KEY,
            encrypted_hash TEXT NOT NULL,
            file_name TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            price_sats INTEGER NOT NULL,
            chunk_size INTEGER NOT NULL DEFAULT 0,
            chunk_count INTEGER NOT NULL DEFAULT 0,
            plaintext_root TEXT NOT NULL DEFAULT '',
            encrypted_root TEXT NOT NULL DEFAULT '',
            creator_pubkey TEXT NOT NULL,
            creator_address TEXT NOT NULL,
            creator_ln_address TEXT NOT NULL,
            registered_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS seeders (
            encrypted_hash TEXT NOT NULL,
            seeder_pubkey TEXT NOT NULL,
            seeder_address TEXT NOT NULL,
            seeder_ln_address TEXT NOT NULL,
            transport_price INTEGER NOT NULL,
            chunk_count INTEGER NOT NULL DEFAULT 0,
            announced_at TEXT NOT NULL,
            PRIMARY KEY (encrypted_hash, seeder_pubkey)
        );

        CREATE INDEX IF NOT EXISTS idx_seeders_enc_hash ON seeders(encrypted_hash);
        CREATE INDEX IF NOT EXISTS idx_listings_enc_hash ON listings(encrypted_hash);
        ",
    )
    .expect("Failed to initialize database schema");
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/listings -- creator publishes a content listing
async fn create_listing(
    State(state): State<AppState>,
    Json(listing): Json<ContentListing>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let result = db.execute(
        "INSERT OR REPLACE INTO listings
         (content_hash, encrypted_hash, file_name, size_bytes, price_sats,
          chunk_size, chunk_count, plaintext_root, encrypted_root,
          creator_pubkey, creator_address, creator_ln_address, registered_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            listing.content_hash,
            listing.encrypted_hash,
            listing.file_name,
            listing.size_bytes,
            listing.price_sats,
            listing.chunk_size,
            listing.chunk_count,
            listing.plaintext_root,
            listing.encrypted_root,
            listing.creator_pubkey,
            listing.creator_address,
            listing.creator_ln_address,
            listing.registered_at,
        ],
    );

    match result {
        Ok(_) => {
            println!(
                "Listing stored: {} ({})",
                listing.file_name, listing.content_hash
            );
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => {
            eprintln!("Failed to store listing: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        }
    }
}

/// GET /api/listings -- list all content listings
async fn list_listings(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let mut stmt = db
        .prepare(
            "SELECT content_hash, encrypted_hash, file_name, size_bytes, price_sats,
                    chunk_size, chunk_count, plaintext_root, encrypted_root,
                    creator_pubkey, creator_address, creator_ln_address, registered_at
             FROM listings ORDER BY registered_at DESC",
        )
        .unwrap();

    let items: Vec<ContentListing> = stmt
        .query_map([], |row| {
            Ok(ContentListing {
                content_hash: row.get(0)?,
                encrypted_hash: row.get(1)?,
                file_name: row.get(2)?,
                size_bytes: row.get(3)?,
                price_sats: row.get(4)?,
                chunk_size: row.get(5)?,
                chunk_count: row.get(6)?,
                plaintext_root: row.get(7)?,
                encrypted_root: row.get(8)?,
                creator_pubkey: row.get(9)?,
                creator_address: row.get(10)?,
                creator_ln_address: row.get(11)?,
                registered_at: row.get(12)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    Json(serde_json::json!({ "items": items }))
}

/// GET /api/listings/{content_hash} -- get a specific listing
async fn get_listing(
    State(state): State<AppState>,
    Path(content_hash): Path<String>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let result = db.query_row(
        "SELECT content_hash, encrypted_hash, file_name, size_bytes, price_sats,
                chunk_size, chunk_count, plaintext_root, encrypted_root,
                creator_pubkey, creator_address, creator_ln_address, registered_at
         FROM listings WHERE content_hash = ?1",
        rusqlite::params![content_hash],
        |row| {
            Ok(ContentListing {
                content_hash: row.get(0)?,
                encrypted_hash: row.get(1)?,
                file_name: row.get(2)?,
                size_bytes: row.get(3)?,
                price_sats: row.get(4)?,
                chunk_size: row.get(5)?,
                chunk_count: row.get(6)?,
                plaintext_root: row.get(7)?,
                encrypted_root: row.get(8)?,
                creator_pubkey: row.get(9)?,
                creator_address: row.get(10)?,
                creator_ln_address: row.get(11)?,
                registered_at: row.get(12)?,
            })
        },
    );

    match result {
        Ok(listing) => (StatusCode::OK, Json(serde_json::json!(listing))).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Listing not found"})),
        )
            .into_response(),
    }
}

/// GET /api/search?q=term&type=mp4&max_price=1000 -- search listings
async fn search_listings(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();

    // Build dynamic query
    let mut sql = String::from(
        "SELECT content_hash, encrypted_hash, file_name, size_bytes, price_sats,
                chunk_size, chunk_count, plaintext_root, encrypted_root,
                creator_pubkey, creator_address, creator_ln_address, registered_at
         FROM listings WHERE 1=1",
    );
    let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(ref q) = params.q {
        sql.push_str(&format!(" AND file_name LIKE ?{}", param_idx));
        bind_values.push(Box::new(format!("%{}%", q)));
        param_idx += 1;
    }

    if let Some(ref content_type) = params.content_type {
        sql.push_str(&format!(" AND file_name LIKE ?{}", param_idx));
        bind_values.push(Box::new(format!("%.{}", content_type)));
        param_idx += 1;
    }

    if let Some(max_price) = params.max_price {
        sql.push_str(&format!(" AND price_sats <= ?{}", param_idx));
        bind_values.push(Box::new(max_price as i64));
        // param_idx += 1;  // last param
    }

    sql.push_str(" ORDER BY registered_at DESC");

    let mut stmt = db.prepare(&sql).unwrap();
    let params_ref: Vec<&dyn rusqlite::types::ToSql> = bind_values.iter().map(|b| b.as_ref()).collect();

    let items: Vec<ContentListing> = stmt
        .query_map(params_ref.as_slice(), |row| {
            Ok(ContentListing {
                content_hash: row.get(0)?,
                encrypted_hash: row.get(1)?,
                file_name: row.get(2)?,
                size_bytes: row.get(3)?,
                price_sats: row.get(4)?,
                chunk_size: row.get(5)?,
                chunk_count: row.get(6)?,
                plaintext_root: row.get(7)?,
                encrypted_root: row.get(8)?,
                creator_pubkey: row.get(9)?,
                creator_address: row.get(10)?,
                creator_ln_address: row.get(11)?,
                registered_at: row.get(12)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    Json(serde_json::json!({ "items": items }))
}

/// POST /api/seeders -- seeder announces availability
async fn create_seeder(
    State(state): State<AppState>,
    Json(announcement): Json<SeederAnnouncement>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let result = db.execute(
        "INSERT OR REPLACE INTO seeders
         (encrypted_hash, seeder_pubkey, seeder_address, seeder_ln_address,
          transport_price, chunk_count, announced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            announcement.encrypted_hash,
            announcement.seeder_pubkey,
            announcement.seeder_address,
            announcement.seeder_ln_address,
            announcement.transport_price,
            announcement.chunk_count,
            announcement.announced_at,
        ],
    );

    match result {
        Ok(_) => {
            println!(
                "Seeder announced: {} for {}",
                announcement.seeder_address, announcement.encrypted_hash
            );
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => {
            eprintln!("Failed to store seeder: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        }
    }
}

/// GET /api/discover/{content_hash} -- listing + all seeders for that content
async fn discover(
    State(state): State<AppState>,
    Path(content_hash): Path<String>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();

    // Get the listing
    let listing_result = db.query_row(
        "SELECT content_hash, encrypted_hash, file_name, size_bytes, price_sats,
                chunk_size, chunk_count, plaintext_root, encrypted_root,
                creator_pubkey, creator_address, creator_ln_address, registered_at
         FROM listings WHERE content_hash = ?1",
        rusqlite::params![content_hash],
        |row| {
            Ok(ContentListing {
                content_hash: row.get(0)?,
                encrypted_hash: row.get(1)?,
                file_name: row.get(2)?,
                size_bytes: row.get(3)?,
                price_sats: row.get(4)?,
                chunk_size: row.get(5)?,
                chunk_count: row.get(6)?,
                plaintext_root: row.get(7)?,
                encrypted_root: row.get(8)?,
                creator_pubkey: row.get(9)?,
                creator_address: row.get(10)?,
                creator_ln_address: row.get(11)?,
                registered_at: row.get(12)?,
            })
        },
    );

    let listing = match listing_result {
        Ok(l) => l,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Listing not found"})),
            )
                .into_response();
        }
    };

    // Get all seeders for this content's encrypted_hash
    let mut stmt = db
        .prepare(
            "SELECT encrypted_hash, seeder_pubkey, seeder_address, seeder_ln_address,
                    transport_price, chunk_count, announced_at
             FROM seeders WHERE encrypted_hash = ?1",
        )
        .unwrap();

    let seeders: Vec<SeederAnnouncement> = stmt
        .query_map(rusqlite::params![listing.encrypted_hash], |row| {
            Ok(SeederAnnouncement {
                encrypted_hash: row.get(0)?,
                seeder_pubkey: row.get(1)?,
                seeder_address: row.get(2)?,
                seeder_ln_address: row.get(3)?,
                transport_price: row.get(4)?,
                chunk_count: row.get(5)?,
                announced_at: row.get(6)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let response = DiscoverResponse { listing, seeders };
    (StatusCode::OK, Json(serde_json::json!(response))).into_response()
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

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
        .route("/api/listings", post(create_listing).get(list_listings))
        .route("/api/listings/{content_hash}", get(get_listing))
        .route("/api/search", get(search_listings))
        .route("/api/seeders", post(create_seeder))
        .route("/api/discover/{content_hash}", get(discover))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cli.port);
    println!("Conduit Registry listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
