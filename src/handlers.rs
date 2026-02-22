//! HTTP handler functions for the Conduit Registry API.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::db::{listing_from_row, LISTING_COLS};
use crate::types::{
    AppState, ContentListing, DiscoverResponse, Manufacturer, SearchParams, SeederAnnouncement,
};

/// POST /api/listings -- creator publishes a content listing
pub async fn create_listing(
    State(state): State<AppState>,
    Json(listing): Json<ContentListing>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let result = db.execute(
        "INSERT OR REPLACE INTO listings
         (content_hash, encrypted_hash, file_name, size_bytes, price_sats,
          chunk_size, chunk_count, plaintext_root, encrypted_root,
          creator_pubkey, creator_address, creator_ln_address, creator_alias, registered_at,
          pre_c1_hex, pre_c2_hex, pre_pk_creator_hex, playback_policy)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
            listing.creator_alias,
            listing.registered_at,
            listing.pre_c1_hex,
            listing.pre_c2_hex,
            listing.pre_pk_creator_hex,
            listing.playback_policy,
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
pub async fn list_listings(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let sql = format!("SELECT {} FROM listings ORDER BY registered_at DESC", LISTING_COLS);
    let mut stmt = db.prepare(&sql).unwrap();

    let items: Vec<ContentListing> = stmt
        .query_map([], listing_from_row)
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    Json(serde_json::json!({ "items": items }))
}

/// GET /api/listings/{content_hash} -- get a specific listing
pub async fn get_listing(
    State(state): State<AppState>,
    Path(content_hash): Path<String>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let sql = format!("SELECT {} FROM listings WHERE content_hash = ?1", LISTING_COLS);
    let result = db.query_row(&sql, rusqlite::params![content_hash], listing_from_row);

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
pub async fn search_listings(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();

    // Build dynamic query
    let mut sql = format!("SELECT {} FROM listings WHERE 1=1", LISTING_COLS);
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
    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        bind_values.iter().map(|b| b.as_ref()).collect();

    let items: Vec<ContentListing> = stmt
        .query_map(params_ref.as_slice(), listing_from_row)
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    Json(serde_json::json!({ "items": items }))
}

/// POST /api/seeders -- seeder announces availability
pub async fn create_seeder(
    State(state): State<AppState>,
    Json(announcement): Json<SeederAnnouncement>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let result = db.execute(
        "INSERT OR REPLACE INTO seeders
         (encrypted_hash, seeder_pubkey, seeder_address, seeder_ln_address, seeder_alias,
          transport_price, chunk_count, announced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            announcement.encrypted_hash,
            announcement.seeder_pubkey,
            announcement.seeder_address,
            announcement.seeder_ln_address,
            announcement.seeder_alias,
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
pub async fn discover(
    State(state): State<AppState>,
    Path(content_hash): Path<String>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();

    // Get the listing
    let sql = format!("SELECT {} FROM listings WHERE content_hash = ?1", LISTING_COLS);
    let listing_result = db.query_row(&sql, rusqlite::params![content_hash], listing_from_row);

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
            "SELECT encrypted_hash, seeder_pubkey, seeder_address, seeder_ln_address, seeder_alias,
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
                seeder_alias: row.get(4)?,
                transport_price: row.get(5)?,
                chunk_count: row.get(6)?,
                announced_at: row.get(7)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let response = DiscoverResponse { listing, seeders };
    (StatusCode::OK, Json(serde_json::json!(response))).into_response()
}

/// DELETE /api/listings -- clear all listings (for test re-provisioning)
pub async fn delete_all_listings(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let deleted = db.execute("DELETE FROM listings", []).unwrap_or(0);
    println!("Cleared {} listings", deleted);
    (StatusCode::OK, Json(serde_json::json!({ "deleted": deleted })))
}

/// DELETE /api/seeders -- clear all seeder announcements (for test re-provisioning)
pub async fn delete_all_seeders(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let deleted = db.execute("DELETE FROM seeders", []).unwrap_or(0);
    println!("Cleared {} seeder announcements", deleted);
    (StatusCode::OK, Json(serde_json::json!({ "deleted": deleted })))
}

// ---------------------------------------------------------------------------
// Manufacturer handlers
// ---------------------------------------------------------------------------

/// POST /api/manufacturers -- register a TEE device manufacturer
pub async fn create_manufacturer(
    State(state): State<AppState>,
    Json(mut mfr): Json<Manufacturer>,
) -> impl IntoResponse {
    if mfr.registered_at.is_empty() {
        mfr.registered_at = chrono::Utc::now().to_rfc3339();
    }
    let db = state.db.lock().unwrap();
    let result = db.execute(
        "INSERT OR REPLACE INTO manufacturers (pk_hex, name, description, website, registered_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![mfr.pk_hex, mfr.name, mfr.description, mfr.website, mfr.registered_at],
    );
    match result {
        Ok(_) => {
            println!("Manufacturer registered: {} ({})", mfr.name, &mfr.pk_hex[..16]);
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => {
            eprintln!("Failed to register manufacturer: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})))
        }
    }
}

/// GET /api/manufacturers -- list all registered manufacturers
pub async fn list_manufacturers(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let mut stmt = db
        .prepare("SELECT pk_hex, name, description, website, registered_at FROM manufacturers ORDER BY registered_at DESC")
        .unwrap();
    let items: Vec<Manufacturer> = stmt
        .query_map([], |row| {
            Ok(Manufacturer {
                pk_hex: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                website: row.get(3)?,
                registered_at: row.get(4)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    Json(serde_json::json!({ "items": items }))
}

/// GET /api/manufacturers/{pk_hex} -- get a specific manufacturer
pub async fn get_manufacturer(
    State(state): State<AppState>,
    Path(pk_hex): Path<String>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let result = db.query_row(
        "SELECT pk_hex, name, description, website, registered_at FROM manufacturers WHERE pk_hex = ?1",
        rusqlite::params![pk_hex],
        |row| {
            Ok(Manufacturer {
                pk_hex: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                website: row.get(3)?,
                registered_at: row.get(4)?,
            })
        },
    );
    match result {
        Ok(mfr) => (StatusCode::OK, Json(serde_json::json!(mfr))).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Manufacturer not found"}))).into_response(),
    }
}

/// DELETE /api/manufacturers/{pk_hex} -- deregister a manufacturer
pub async fn delete_manufacturer(
    State(state): State<AppState>,
    Path(pk_hex): Path<String>,
) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let deleted = db
        .execute("DELETE FROM manufacturers WHERE pk_hex = ?1", rusqlite::params![pk_hex])
        .unwrap_or(0);
    if deleted > 0 {
        println!("Manufacturer deregistered: {}", &pk_hex[..16.min(pk_hex.len())]);
        (StatusCode::OK, Json(serde_json::json!({"ok": true, "deleted": deleted})))
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Manufacturer not found"})))
    }
}

/// DELETE /api/manufacturers -- clear all manufacturers (test re-provisioning)
pub async fn delete_all_manufacturers(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let deleted = db.execute("DELETE FROM manufacturers", []).unwrap_or(0);
    println!("Cleared {} manufacturers", deleted);
    (StatusCode::OK, Json(serde_json::json!({ "deleted": deleted })))
}

// ---------------------------------------------------------------------------
// Seeders list (for dashboard)
// ---------------------------------------------------------------------------

pub async fn list_seeders(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().unwrap();
    let mut stmt = db
        .prepare(
            "SELECT encrypted_hash, seeder_pubkey, seeder_address, seeder_ln_address, seeder_alias,
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
                seeder_alias: row.get(4)?,
                transport_price: row.get(5)?,
                chunk_count: row.get(6)?,
                announced_at: row.get(7)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    Json(serde_json::json!({ "items": items }))
}
