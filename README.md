# Sheffield Family Calendar & Routine Hub

A local-first family smart calendar, morning routine tracker and collaborative
whiteboard. The kiosk view targets a Fire OS tablet in kiosk mode; phones use
the same server as a companion PWA.

## Stack

| Layer      | Choice                                                  |
| ---------- | ------------------------------------------------------- |
| Framework  | Dioxus 0.6 fullstack (`router` + `fullstack`)            |
| Server     | tokio + axum + tower-http (behind the `server` feature)  |
| Database   | SQLite via sqlx (behind the `server` feature)            |
| Styling    | Tailwind CSS with the Sheffield palette                  |
| Realtime   | Axum WebSocket at `/ws` backed by `tokio::sync::broadcast` |

### Sheffield palette

| Token               | Hex       |
| ------------------- | --------- |
| `sheffield-light`   | `#8BB5DA` |
| `sheffield-dark`    | `#2672B3` |
| `sheffield-accent`  | `#E86A58` |
| `sheffield-sun`     | `#F4D03F` |
| `sheffield-paper`   | `#FDFDFD` |

## Layout

```
src/
  main.rs              server entrypoint (axum) / web entrypoint
  shared/types.rs      types shared by client and server
  client/app.rs        router, global signals, page shells
  client/components/   dashboard, routine, calendar, whiteboard, screensaver
  server/api.rs        #[server] functions + the /ws broadcast route
  server/db.rs         SQLite pool, schema, seeds
  server/calendar.rs   Google Calendar polling task
tests/db_tests.rs      database integration tests
```

## Toolchain

```bash
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli --version 0.6.3 --locked   # provides `dx`
curl -sLO https://github.com/tailwindlabs/tailwindcss/releases/download/v3.4.17/tailwindcss-linux-x64
chmod +x tailwindcss-linux-x64 && sudo mv tailwindcss-linux-x64 /usr/local/bin/tailwindcss
```

`libssl-dev` and `pkg-config` are required for the native-tls build of sqlx.

## Running

```bash
tailwindcss -i input.css -o assets/tailwind.css --watch   # in one shell
dx serve --platform web                                   # in another
```

The kiosk dashboard is served at `/`, the phone companion at `/mobile`.

## Configuration

| Variable                       | Purpose                                              |
| ------------------------------ | ---------------------------------------------------- |
| `DATABASE_URL`                 | SQLite URL, defaults to `sqlite://family.db`          |
| `GOOGLE_SERVICE_ACCOUNT_JSON`  | Path to a service account key; unset disables polling |
| `GOOGLE_CALENDAR_ID`           | Calendar to poll, defaults to `primary`               |

Drop family photos into `assets/screensaver/` for the ambient slideshow that
takes over after 10 minutes of inactivity.

## Tests

```bash
cargo test --features server
cargo clippy --features server --all-targets
cargo clippy --features web --target wasm32-unknown-unknown
```
