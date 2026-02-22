//! Data types for the Conduit Registry API.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContentListing {
    pub content_hash: String,
    pub encrypted_hash: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub price_sats: u64,
    pub chunk_size: u64,
    pub chunk_count: u64,
    pub plaintext_root: String,
    pub encrypted_root: String,
    pub creator_pubkey: String,
    pub creator_address: String,
    pub creator_ln_address: String,
    pub creator_alias: String,
    pub registered_at: String,
    #[serde(default)]
    pub pre_c1_hex: String,
    #[serde(default)]
    pub pre_c2_hex: String,
    #[serde(default)]
    pub pre_pk_creator_hex: String,
    #[serde(default = "default_playback_policy")]
    pub playback_policy: String,
    #[serde(default)]
    pub creator_signature: String,
}

pub fn default_playback_policy() -> String {
    "open".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SeederAnnouncement {
    pub encrypted_hash: String,
    pub seeder_pubkey: String,
    pub seeder_address: String,
    pub seeder_ln_address: String,
    pub seeder_alias: String,
    pub transport_price: u64,
    pub chunk_count: u64,
    pub announced_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manufacturer {
    pub pk_hex: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub registered_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
    #[serde(rename = "type")]
    pub content_type: Option<String>,
    pub max_price: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub listing: ContentListing,
    pub seeders: Vec<SeederAnnouncement>,
}
