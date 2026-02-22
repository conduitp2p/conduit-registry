# Conduit Registry

**Content discovery registry for the Conduit network.**

A lightweight Axum + SQLite service that acts as the centralized content index.
Creators publish listings here; buyers query it to find content and discover
which peers are seeding each file. Designed to be replaceable by a
decentralized alternative (Nostr relays, DHT) once the protocol stabilizes.

## API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/api/listings` | Register a content listing |
| `GET` | `/api/listings` | List all content |
| `GET` | `/api/listings/{content_hash}` | Get a single listing |
| `GET` | `/api/search?q=...` | Full-text search across listings |
| `DELETE` | `/api/listings` | Clear all listings (admin) |
| `POST` | `/api/seeders` | Announce seeder availability for a content hash |
| `GET` | `/api/seeders` | List all seeder announcements |
| `DELETE` | `/api/seeders` | Clear all seeder records (admin) |
| `GET` | `/api/discover/{content_hash}` | Discover all sources (creator + seeders) for content |
| `POST` | `/api/manufacturers` | Register a TEE device manufacturer |
| `GET` | `/api/manufacturers` | List registered manufacturers |
| `GET` | `/api/manufacturers/{pk_hex}` | Get manufacturer by public key |
| `DELETE` | `/api/manufacturers/{pk_hex}` | Remove a manufacturer |
| `GET` | `/` | HTML dashboard with live listing table |

## Build and run

```bash
cargo build --release
./target/release/conduit-registry --port 3003 --db registry.sqlite
```

Or with cargo:

```bash
cargo run -- --port 3003 --db registry.sqlite
```

### CLI flags

| Flag | Default | Description |
|------|---------|-------------|
| `--port` | `3003` | HTTP listen port |
| `--db` | `registry.sqlite` | SQLite database path |

## Deployment

Push to `main` on `conduitp2p/conduit-registry` triggers a GitHub Actions
workflow that builds on ubuntu x86_64, SCPs the binary to the registry
droplet, and runs `systemctl restart conduit-registry`.

Currently hosted on `157.230.238.79:3003` (shared droplet with Seeder 2).

## Project structure

```
conduit-registry/
├── src/
│   ├── main.rs        Entry point, CLI, router setup
│   ├── types.rs       Data models (ContentListing, SeederAnnouncement, etc.)
│   ├── db.rs          SQLite schema, migrations, query helpers
│   ├── handlers.rs    HTTP handler functions
│   └── dashboard.rs   Inline HTML dashboard
└── Cargo.toml
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
