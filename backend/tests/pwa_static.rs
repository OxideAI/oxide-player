//! Verifies the static-file MIME types the backend serves for PWA assets.
//!
//! The backend serves `frontend/dist/` through `tower_http::ServeDir`, which
//! derives content types from `mime_guess`. These tests confirm the manifest,
//! service worker and icons are served with the MIME types browsers require
//! for installability and SW registration.
//!
//! Run with: `cargo test --test pwa_static` (requires a `frontend/dist`
//! produced by `npm run build` — CI must build the frontend first, otherwise
//! these tests fail rather than silently passing with no assertions).
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use tower_http::services::ServeDir;

fn dist_dir() -> Option<std::path::PathBuf> {
    let candidates = [
        "frontend/dist",
        "../frontend/dist",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../frontend/dist"),
    ];
    candidates
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("sw.js").is_file())
}

fn require_dist() -> std::path::PathBuf {
    match dist_dir() {
        Some(d) => d,
        None => panic!(
            "frontend/dist not found (looked for sw.js). Run `npm run build` in frontend/ before cargo test.",
        ),
    }
}

async fn serve(path: &str) -> Option<(StatusCode, String, String)> {
    let dir = dist_dir()?;
    let svc = ServeDir::new(&dir).append_index_html_on_directories(false);
    let req = Request::builder()
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let resp = svc.oneshot(req).await.ok()?;
    let ct = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let status = resp.status();
    let body = String::from_utf8_lossy(
        &axum::body::to_bytes(axum::body::Body::new(resp.into_body()), usize::MAX)
            .await
            .ok()?,
    )
    .to_string();
    Some((status, ct, body))
}

#[tokio::test]
async fn manifest_served_as_manifest_json() {
    require_dist();
    let Some((status, ct, _)) = serve("/manifest.webmanifest").await else {
        panic!("manifest.webmanifest not served");
    };
    assert_eq!(status, StatusCode::OK);
    assert!(
        ct == "application/manifest+json" || ct.starts_with("application/manifest+json"),
        "manifest content-type was {ct}"
    );
}

#[tokio::test]
async fn service_worker_served_as_javascript() {
    require_dist();
    let Some((status, ct, _)) = serve("/sw.js").await else {
        panic!("sw.js not served");
    };
    assert_eq!(status, StatusCode::OK);
    assert!(
        ct == "text/javascript" || ct == "application/javascript" || ct.starts_with("text/javascript") || ct.starts_with("application/javascript"),
        "service worker content-type was {ct}"
    );
}

#[tokio::test]
async fn manifest_points_at_existing_icons() {
    require_dist();
    let Some((status, _ct, body)) = serve("/manifest.webmanifest").await else {
        panic!("manifest.webmanifest not served");
    };
    assert_eq!(status, StatusCode::OK);
    let manifest: serde_json::Value = serde_json::from_str(&body).expect("manifest is valid JSON");
    assert_eq!(manifest["name"], "Oxide Player");
    assert_eq!(manifest["display"], "standalone");
    let icons = manifest["icons"].as_array().expect("icons array");
    assert!(!icons.is_empty());
    for icon in icons {
        let src = icon["src"].as_str().expect("icon src");
        let path = if src.starts_with('/') { src.to_string() } else { format!("/{src}") };
        let (status, _, icon_body) = serve(&path).await.expect("icon reachable");
        assert_eq!(status, StatusCode::OK, "icon {src} not served (status {status})");
        assert!(!icon_body.is_empty(), "icon {src} is empty");
    }
}

#[test]
fn mime_guess_maps_pwa_extensions() {
    assert_eq!(
        mime_guess::from_path("x.webmanifest").first_or_octet_stream(),
        "application/manifest+json"
    );
    let js = mime_guess::from_path("x.js").first_or_octet_stream();
    assert!(
        js.as_ref() == "text/javascript" || js.as_ref() == "application/javascript"
    );
}
