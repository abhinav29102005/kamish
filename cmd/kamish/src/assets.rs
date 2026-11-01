use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;
use std::path::PathBuf;

const INDEX_HTML: &str = "index.html";

#[derive(RustEmbed)]
#[folder = "../../frontend/dist"]
#[exclude = "models/*.glb"]
pub struct EmbeddedAssets;

fn apply_csp_and_security_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' ws: wss:; frame-ancestors 'none';"
        ),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
}

/// Fallback static file handler for the embedded dashboard and local models
pub async fn static_handler(uri: Uri) -> Response {
    let raw_path = uri.path().trim_start_matches('/');
    let path = if raw_path.is_empty() {
        INDEX_HTML
    } else {
        raw_path
    };

    // 1. Allow disk fallback for 3D models if present on local filesystem
    if path.starts_with("models/") {
        let model_filename = path.strip_prefix("models/").unwrap_or(path);
        let candidate_dirs = [
            PathBuf::from("frontend/public/models"),
            PathBuf::from("frontend/dist/models"),
            PathBuf::from("models"),
            PathBuf::from("./models"),
        ];

        for dir in &candidate_dirs {
            let file_path = dir.join(model_filename);
            if file_path.is_file()
                && let Ok(bytes) = tokio::fs::read(&file_path).await
            {
                let mut headers = HeaderMap::new();
                apply_csp_and_security_headers(&mut headers);
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("model/gltf-binary"),
                );
                headers.insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=3600"),
                );
                return (StatusCode::OK, headers, Body::from(bytes)).into_response();
            }
        }
    }

    // 2. Try embedded assets
    if let Some(content) = EmbeddedAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let mut headers = HeaderMap::new();
        apply_csp_and_security_headers(&mut headers);

        if let Ok(val) = HeaderValue::from_str(mime.as_ref()) {
            headers.insert(header::CONTENT_TYPE, val);
        }

        // Immutable caching for hashed assets, no-cache for index.html
        if path.starts_with("assets/") {
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
        } else if path == INDEX_HTML {
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache, no-store, must-revalidate"),
            );
        } else {
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=3600"),
            );
        }

        return (StatusCode::OK, headers, Body::from(content.data)).into_response();
    }

    // 3. SPA Fallback: if not found and path does not contain an extension, serve index.html
    if !path.contains('.')
        && let Some(index) = EmbeddedAssets::get(INDEX_HTML)
    {
        let mut headers = HeaderMap::new();
        apply_csp_and_security_headers(&mut headers);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        );
        return (StatusCode::OK, headers, Body::from(index.data)).into_response();
    }

    // 4. Not found
    let mut headers = HeaderMap::new();
    apply_csp_and_security_headers(&mut headers);
    (StatusCode::NOT_FOUND, headers, Body::from("404 Not Found")).into_response()
}
