# 17Lands desktop

A Tauri v2 menu-bar / notification-area app that wraps the rust log follower core
The follower runs on a background thread, shows a live log feed in an on-demand window, 
and captures the 17Lands token through a GUI instead of the CLI's stdin prompt.

## Architecture

| File | Role |
|------|------|
| `src/main.rs` | App bootstrap, tray (menu-bar, accessory mode), window toggle, command registration |
| `src/state.rs` | `AppState`: follower thread + cancellation, resolved log path, shared upload status |
| `src/observer.rs` | `ObservingSubmitter` — records endpoint/time/count without altering POSTs |
| `src/logbridge.rs` | Global `log::Log` sink → ring buffer + `log-line` webview events |
| `src/commands.rs` | `#[tauri::command]` bridge (token, status, start/stop, log path) |
| `ui/` | Vanilla HTML/JS/CSS frontend (no bundler; uses `withGlobalTauri`) |

## Develop

```sh
# Never POST to the live API during dev — point at the local oracle mock.
python3 ../tools/oracle/mock_server.py 8732 /tmp/desktop-out.jsonl &
SEVENTEENLANDS_HOST=http://127.0.0.1:8732 cargo tauri dev
```

Dev/test env overrides (parallel to each other):
- `SEVENTEENLANDS_HOST` — upload host (default: live API). **Always set this to the mock in dev.**
- `SEVENTEENLANDS_LOG` — pin the followed log file (e.g. `../tests/fixtures/gaps.log`) so the app
  can run headlessly without a real `Player.log`.

## Build a bundle

```sh
cargo tauri build # produces .app + .dmg under target/release/bundle/
```

## TODO

- Code signing / notarization
- Windows (`.msi`/NSIS) targets can be added to `tauri.conf.json` later.
