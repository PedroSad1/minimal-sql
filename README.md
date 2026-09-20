# Graphite

SQL client forked from Beekeeper Studio **Community** (GPLv3), rewritten with
Tauri 2 + Rust. All Graphite features are free. There is no license key.

Graphite is an independent project. It is not Beekeeper Studio.

## What is in this repo

- Vue UI shell with the Community layout tokens (black / gray palette)
- Tauri commands that keep the `conn/*` and `query/*` names
- Rust drivers for Community databases: SQLite, PostgreSQL (and CockroachDB,
  Redshift, GreengageDB), MySQL (and MariaDB, TiDB, StarRocks, Bedrock),
  SQL Server, Redis, BigQuery
- SSH tunnel (`russh`)
- Local appdb (`rusqlite`)
- Extras written from public crates: N+ filters, JSON viewer, editable
  results, execute-to-file, import CSV/JSON/XLSX

This repository does **not** contain `src-commercial`.

## Dev

Rust 1.98+ and Node 24+ are required.

```bash
cd apps/desktop
pnpm install
pnpm tauri dev
```

## Tests

SQLite tests always run:

```bash
cargo test -p graphite-core -p graphite-drivers -p graphite-appdb
```

Docker integration (Postgres, MySQL, Redis, SQL Server):

```bash
docker compose -f docker-compose.test.yml up -d
GRAPHITE_DOCKER=1 cargo test -p graphite-drivers -- --ignored --nocapture
```

## Git remote

Push with the personal SSH host:

```bash
git remote add origin git@github.com-personal:PedroSad1/graphite.git
```
