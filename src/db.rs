//! Database initialization and helpers for the Conduit Registry.

use rusqlite::Connection;

use crate::types::ContentListing;

pub fn init_db(conn: &Connection) {
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
            creator_alias TEXT NOT NULL DEFAULT '',
            registered_at TEXT NOT NULL,
            creator_signature TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS seeders (
            encrypted_hash TEXT NOT NULL,
            seeder_pubkey TEXT NOT NULL,
            seeder_address TEXT NOT NULL,
            seeder_ln_address TEXT NOT NULL,
            seeder_alias TEXT NOT NULL DEFAULT '',
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

    // Migration: add alias columns to existing databases
    let _ = conn.execute(
        "ALTER TABLE listings ADD COLUMN creator_alias TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE seeders ADD COLUMN seeder_alias TEXT NOT NULL DEFAULT ''",
        [],
    );
    // Migration: add PRE columns
    let _ = conn.execute(
        "ALTER TABLE listings ADD COLUMN pre_c1_hex TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE listings ADD COLUMN pre_c2_hex TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE listings ADD COLUMN pre_pk_creator_hex TEXT NOT NULL DEFAULT ''",
        [],
    );
    // Migration: add playback_policy column
    let _ = conn.execute(
        "ALTER TABLE listings ADD COLUMN playback_policy TEXT NOT NULL DEFAULT 'open'",
        [],
    );
    // Migration: add creator_signature column (Layer 2 signed listings)
    let _ = conn.execute(
        "ALTER TABLE listings ADD COLUMN creator_signature TEXT NOT NULL DEFAULT ''",
        [],
    );
    // TEE device manufacturers table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS manufacturers (
            pk_hex TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            website TEXT NOT NULL DEFAULT '',
            registered_at TEXT NOT NULL
        );"
    ).expect("Failed to create manufacturers table");
}

pub fn listing_from_row(row: &rusqlite::Row) -> rusqlite::Result<ContentListing> {
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
        creator_alias: row.get(12)?,
        registered_at: row.get(13)?,
        pre_c1_hex: row.get(14)?,
        pre_c2_hex: row.get(15)?,
        pre_pk_creator_hex: row.get(16)?,
        playback_policy: row.get(17)?,
        creator_signature: row.get(18)?,
    })
}

pub const LISTING_COLS: &str =
    "content_hash, encrypted_hash, file_name, size_bytes, price_sats,
     chunk_size, chunk_count, plaintext_root, encrypted_root,
     creator_pubkey, creator_address, creator_ln_address, creator_alias, registered_at,
     pre_c1_hex, pre_c2_hex, pre_pk_creator_hex, playback_policy, creator_signature";
