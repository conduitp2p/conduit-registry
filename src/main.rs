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
use axum::response::{Html, IntoResponse};
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
// Dashboard (HTML)
// ---------------------------------------------------------------------------

async fn dashboard() -> Html<&'static str> {
    Html(r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Conduit Registry</title>
<link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>⚡</text></svg>">
<style>
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
         background: #0a0a0a; color: #e0e0e0; min-height: 100vh; }
  header { padding: 2rem 2rem 1rem; border-bottom: 1px solid #222; }
  header h1 { font-size: 1.4rem; font-weight: 600; color: #f7931a; letter-spacing: 0.03em; }
  header p { color: #888; font-size: 0.85rem; margin-top: 0.3rem; }
  .stats { display: flex; gap: 2rem; padding: 1.2rem 2rem; border-bottom: 1px solid #181818; }
  .stat { display: flex; flex-direction: column; }
  .stat-val { font-size: 1.6rem; font-weight: 700; color: #fff; }
  .stat-label { font-size: 0.75rem; color: #666; text-transform: uppercase; letter-spacing: 0.06em; }
  main { padding: 1.5rem 2rem; }
  .empty { color: #555; font-style: italic; padding: 3rem 0; text-align: center; }
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th { text-align: left; color: #666; font-weight: 500; padding: 0.6rem 0.8rem;
       border-bottom: 1px solid #222; text-transform: uppercase; font-size: 0.72rem;
       letter-spacing: 0.05em; }
  td { padding: 0.7rem 0.8rem; border-bottom: 1px solid #151515; vertical-align: top; }
  tr:hover td { background: #111; }
  .name { color: #fff; font-weight: 500; }
  .hash { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.78rem; color: #666; }
  .price { color: #f7931a; font-weight: 600; white-space: nowrap; }
  .size { color: #aaa; }
  .chunks { color: #aaa; }
  .creator { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.72rem; color: #555; }
  .seeders-badge { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 9999px;
                   font-size: 0.72rem; font-weight: 600; }
  .seeders-0 { background: #1a1a1a; color: #555; }
  .seeders-n { background: #1a2f1a; color: #4ade80; }
  .merkle { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.7rem; color: #444; }
  @media (max-width: 900px) {
    .hide-mobile { display: none; }
    .stats { flex-wrap: wrap; gap: 1rem; }
  }
</style>
</head>
<body>
<header>
  <h1>Conduit Registry</h1>
  <p>Content discovery index</p>
</header>
<div class="stats">
  <div class="stat"><span class="stat-val" id="listing-count">-</span><span class="stat-label">Listings</span></div>
  <div class="stat"><span class="stat-val" id="seeder-count">-</span><span class="stat-label">Seeder Announcements</span></div>
</div>
<main id="content"><p class="empty">Loading...</p></main>
<script>
async function load() {
  const [listRes, seederRes] = await Promise.all([
    fetch('/api/listings').then(r => r.json()),
    fetch('/api/seeders?all=1').then(r => r.json()).catch(() => ({items:[]}))
  ]);
  const listings = listRes.items || [];
  const seeders = seederRes.items || [];

  document.getElementById('listing-count').textContent = listings.length;
  document.getElementById('seeder-count').textContent = seeders.length;

  const main = document.getElementById('content');
  if (!listings.length) { main.innerHTML = '<p class="empty">No content registered yet.</p>'; return; }

  // Count seeders per encrypted_hash
  const seederMap = {};
  seeders.forEach(s => { seederMap[s.encrypted_hash] = (seederMap[s.encrypted_hash]||0) + 1; });

  function short(h) { return h ? h.slice(0,8) + '...' + h.slice(-6) : '-'; }
  function fmtSize(b) {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b/1024).toFixed(1) + ' KB';
    return (b/1048576).toFixed(1) + ' MB';
  }

  let html = `<table>
    <tr>
      <th>Name</th>
      <th>Price</th>
      <th>Size</th>
      <th>Chunks</th>
      <th>Seeders</th>
      <th class="hide-mobile">Creator</th>
      <th class="hide-mobile">Content Hash</th>
    </tr>`;

  listings.forEach(l => {
    const sc = seederMap[l.encrypted_hash] || 0;
    const badge = sc > 0
      ? `<span class="seeders-badge seeders-n">${sc}</span>`
      : `<span class="seeders-badge seeders-0">0</span>`;
    html += `<tr>
      <td class="name">${l.file_name}</td>
      <td class="price">${l.price_sats} sats</td>
      <td class="size">${fmtSize(l.size_bytes)}</td>
      <td class="chunks">${l.chunk_count}</td>
      <td>${badge}</td>
      <td class="creator hide-mobile" title="${l.creator_pubkey}">${short(l.creator_pubkey)}</td>
      <td class="hash hide-mobile" title="${l.content_hash}">${short(l.content_hash)}</td>
    </tr>`;
  });
  html += '</table>';
  main.innerHTML = html;
}
load();
setInterval(load, 10000);
</script>
</body>
</html>"##)
}

// ---------------------------------------------------------------------------
// Seeders list (for dashboard)
// ---------------------------------------------------------------------------

async fn list_seeders(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let mut stmt = db
        .prepare(
            "SELECT encrypted_hash, seeder_pubkey, seeder_address, seeder_ln_address,
                    transport_price, chunk_count, announced_at
             FROM seeders ORDER BY announced_at DESC",
        )
        .unwrap();

    let items: Vec<SeederAnnouncement> = stmt
        .query_map([], |row| {
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

    Json(serde_json::json!({ "items": items }))
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
        .route("/", get(dashboard))
        .route("/api/listings", post(create_listing).get(list_listings))
        .route("/api/listings/{content_hash}", get(get_listing))
        .route("/api/search", get(search_listings))
        .route("/api/seeders", post(create_seeder).get(list_seeders))
        .route("/api/discover/{content_hash}", get(discover))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cli.port);
    println!("Conduit Registry listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
