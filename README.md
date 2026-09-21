# Minimal SQL

SQL client **inspired by** Beekeeper Studio Community (GPLv3), rewritten
from scratch with Tauri 2 + Rust. All Minimal SQL features are free. There is
no license key.

Minimal SQL is an independent project. It is not Beekeeper Studio. This git
tree is not a fork of `beekeeper-studio/beekeeper-studio`.

## What was taken from Community (ideas, not a file copy)

- Database list (SQLite, PostgreSQL family, MySQL family, SQL Server, Redis, BigQuery)
- IPC-style command names (`conn/*`, `query/*`)
- Layout tokens (sidebar + SQL editor + results), with a black / gray palette

## What was not ported

Beekeeper `apps/studio` (TabQueryEditor, SqlTextEditor, Tabulator, Vuex tabs)
is not in this repo. The desktop UI is a small Vue shell over Rust drivers.

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
