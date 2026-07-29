# my-furniture-buyer-app

A buyer's web app for a furniture shop: log in, browse the catalogue, and place
orders against a spending budget. Built for a one-day hackathon.

- `backend/` — Rust API: Actix-web + SQLx + SQLite
- `frontend/` — React + TypeScript SPA (Vite)

## Docs

- [requirements.md](requirements.md) — what it must do, what it deliberately doesn't, and the demo acceptance script
- [architecture.md](architecture.md) — how it's built and why, with the design decisions and their trade-offs
- [CLAUDE.md](CLAUDE.md) — working conventions and the API surface, for anyone (or anything) editing the code

## Prerequisites

- Rust (stable) — <https://rustup.rs>
- Node 20+ — <https://nodejs.org>

## Run it

```bash
# terminal 1 — API on http://127.0.0.1:8080
cd backend
cp .env.example .env
cargo run

# terminal 2 — UI on http://localhost:5173
cd frontend
cp .env.example .env
npm install
npm run dev
```

The SQLite database is created and migrated on first boot, with a seeded
catalogue and a demo buyer:

| email               | password      | budget    |
| ------------------- | ------------- | --------- |
| `buyer@example.com` | `password123` | $5,000.00 |

Open <http://localhost:5173>; the credentials are pre-filled on the login form.
