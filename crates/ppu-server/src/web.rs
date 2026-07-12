use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};
use crate::state::AppState;

pub fn routes(state: &AppState) -> Router<AppState> {
    let dir = &state.cfg.web_dir;
    let index = dir.join("index.html");
    // `not_found_service` would force a 404 status on the injected index.html;
    // `fallback` serves it with the SPA's normal 200 so client-side routing works.
    let serve = ServeDir::new(dir).fallback(ServeFile::new(index));
    Router::new().route("/t/{id}", get(permalink)).fallback_service(serve)
}

async fn permalink(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let index_path = state.cfg.web_dir.join("index.html");
    let html = match tokio::fs::read_to_string(&index_path).await { Ok(s) => s, Err(_) => return (StatusCode::NOT_FOUND, "not built").into_response() };
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT t.title, u.handle, u.avatar_hash FROM toys t JOIN users u ON u.id=t.author_id WHERE t.id=?"
    ).bind(&id).fetch_optional(&state.pool).await.ok().flatten();
    let injected = match row {
        Some((title, handle, _)) => {
            let esc = |s: &str| s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;");
            let og = format!(
                "<meta property=\"og:title\" content=\"{}\">\n<meta property=\"og:description\" content=\"by {}\">\n<meta property=\"og:image\" content=\"{}/blobs/thumb/{}\">\n<meta property=\"og:type\" content=\"video.other\">",
                esc(&title), esc(&handle), state.cfg.base_url, id);
            if html.contains("<!--OG-->") { html.replace("<!--OG-->", &og) }
            else { html.replacen("<head>", &format!("<head>\n{og}"), 1) }
        }
        None => html,
    };
    Html(injected).into_response()
}
