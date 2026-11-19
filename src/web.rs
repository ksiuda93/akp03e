//! Lokalny panel konfiguracyjny: http://127.0.0.1:7878

use std::{convert::Infallible, io::Cursor, sync::Arc};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{
        Html, IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};

use crate::{
    config::{Action, Config},
    device::execute,
    render::render_key,
    state::AppState,
};

pub const ADDR: &str = "127.0.0.1:7878";

type Shared = Arc<AppState>;

pub async fn serve(state: Shared) {
    let app = Router::new()
        .route("/", get(|| async { Html(include_str!("panel.html")) }))
        .route("/api/config", get(get_config).put(put_config))
        .route("/api/state", get(|State(s): State<Shared>| async move { Json(s.status()) }))
        .route("/api/profile/{index}", post(set_profile))
        .route("/api/preview", get(preview))
        .route("/api/icons", post(upload_icon))
        .route("/api/test", post(test_action))
        .route("/api/autostart/{enabled}", post(set_autostart))
        .route(
            "/api/reconnect",
            post(|State(s): State<Shared>| async move {
                s.log("info", "Ponowne łączenie z kontrolerem (panel)");
                s.reconnect.notify_one();
                Json(s.status())
            }),
        )
        .route("/api/events", get(events))
        .with_state(state.clone());

    // Poprzednia instancja może jeszcze przez chwilę trzymać port – próbujemy kilka razy.
    let mut attempt = 0;
    let listener = loop {
        match tokio::net::TcpListener::bind(ADDR).await {
            Ok(l) => break l,
            Err(e) if attempt < 10 => {
                if attempt == 0 {
                    state.log("info", format!("Port {ADDR} zajęty ({e}) – ponawiam…"));
                }
                attempt += 1;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(e) => {
                state.log("error", format!("Nie można uruchomić panelu na {ADDR}: {e}. Czy program nie działa już w innym oknie?"));
                return;
            }
        }
    };
    let mut quit = state.quit.subscribe();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = quit.changed().await;
        })
        .await
        .ok();
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({ "error": msg.into() }))).into_response()
}

async fn get_config(State(s): State<Shared>) -> Json<Config> {
    Json(s.config())
}

async fn put_config(State(s): State<Shared>, headers: axum::http::HeaderMap, Json(mut cfg): Json<Config>) -> Response {
    cfg.normalize();
    if let Err(e) = cfg.save(&s.config_path) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, format!("Zapis nie powiódł się: {e}"));
    }
    *s.config.write().unwrap() = cfg.clone();
    // Inne otwarte panele muszą pobrać nową wersję, inaczej nadpiszą ją swoją starą kopią.
    let client = headers.get("x-client").and_then(|v| v.to_str().ok()).unwrap_or("");
    s.emit(serde_json::json!({ "type": "config", "client": client }));
    // Profil mógł zostać usunięty – set_profile przytnie indeks i odświeży urządzenie.
    s.set_profile(s.profile.load(std::sync::atomic::Ordering::Relaxed));
    Json(cfg).into_response()
}

async fn set_profile(State(s): State<Shared>, Path(index): Path<usize>) -> Json<Value> {
    s.set_profile(index);
    Json(s.status())
}

#[derive(Deserialize)]
struct PreviewQuery {
    profile: usize,
    key: usize,
}

async fn preview(State(s): State<Shared>, Query(q): Query<PreviewQuery>) -> Response {
    let key = {
        let cfg = s.config.read().unwrap();
        match cfg.profiles.get(q.profile).and_then(|p| p.keys.get(q.key)) {
            Some(k) => k.clone(),
            None => return err(StatusCode::NOT_FOUND, "brak klawisza"),
        }
    };
    let img = render_key(&key, &s.icons_dir, s.mic_muted.load(std::sync::atomic::Ordering::Relaxed));
    let mut png = Vec::new();
    if let Err(e) = img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "no-store")], png).into_response()
}

#[derive(Deserialize)]
struct UploadQuery {
    name: String,
}

/// Przyjmuje dowolny obrazek (PNG/JPEG), zmniejsza do 128 px i zapisuje jako PNG w katalogu ikon.
async fn upload_icon(State(s): State<Shared>, Query(q): Query<UploadQuery>, body: Bytes) -> Response {
    let img = match image::load_from_memory(&body) {
        Ok(img) => img,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("To nie jest obsługiwany obrazek (PNG/JPEG): {e}")),
    };
    let img = if img.width() > 128 || img.height() > 128 {
        img.resize(128, 128, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let stem: String = std::path::Path::new(&q.name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("ikona")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '_' })
        .take(40)
        .collect();
    let mut name = format!("{stem}.png");
    let mut n = 2;
    while s.icons_dir.join(&name).exists() {
        name = format!("{stem}-{n}.png");
        n += 1;
    }

    if let Err(e) = img.save_with_format(s.icons_dir.join(&name), image::ImageFormat::Png) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    Json(json!({ "name": name })).into_response()
}

async fn set_autostart(State(s): State<Shared>, Path(enabled): Path<bool>) -> Json<Value> {
    s.set_autostart(enabled);
    Json(s.status())
}

async fn test_action(State(s): State<Shared>, Json(action): Json<Action>) -> Json<Value> {
    execute(&s, action);
    Json(json!({ "ok": true }))
}

async fn events(State(s): State<Shared>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let initial = tokio_stream::once(s.status().to_string());
    let updates = BroadcastStream::new(s.events.subscribe()).filter_map(|m| m.ok());
    let stream = initial.chain(updates).map(|data| Ok(Event::default().data(data)));
    Sse::new(stream).keep_alive(KeepAlive::default())
}
