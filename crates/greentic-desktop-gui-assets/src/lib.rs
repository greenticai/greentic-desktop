mod generated {
    include!(concat!(env!("OUT_DIR"), "/assets.rs"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuiAsset {
    pub path: &'static str,
    pub content_type: &'static str,
    pub bytes: &'static [u8],
    pub etag: &'static str,
}

pub fn asset(path: &str) -> Option<GuiAsset> {
    let path = normalize_path(path);
    let path = if path == "/" { "/index.html" } else { path };
    generated::EMBEDDED_ASSETS
        .iter()
        .find(|asset| asset.path == path)
        .map(|asset| GuiAsset {
            path: asset.path,
            content_type: asset.content_type,
            bytes: asset.bytes,
            etag: asset.etag,
        })
}

pub fn spa_asset(path: &str) -> GuiAsset {
    asset(path).unwrap_or_else(index_asset)
}

pub fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("ico") => "image/x-icon",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("map") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("wasm") => "application/wasm",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

pub fn asset_manifest() -> Vec<&'static str> {
    generated::EMBEDDED_ASSETS
        .iter()
        .map(|asset| asset.path)
        .collect()
}

fn index_asset() -> GuiAsset {
    asset("/index.html").expect("embedded GUI index.html")
}

fn normalize_path(path: &str) -> &str {
    let without_query = path.split_once('?').map_or(path, |(path, _)| path);
    if without_query.is_empty() {
        "/"
    } else if without_query.starts_with('/') {
        without_query
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_index_for_root_and_spa_routes() {
        let root = asset("/").expect("root asset");
        assert_eq!(root.content_type, "text/html; charset=utf-8");
        assert!(std::str::from_utf8(root.bytes)
            .expect("html")
            .contains("Greentic Automate Hub"));

        let route = spa_asset("/create");
        assert_eq!(route.path, "/index.html");
    }

    #[test]
    fn serves_styles_and_known_content_types() {
        let manifest = asset_manifest();
        assert!(manifest
            .iter()
            .filter_map(|path| asset(path))
            .any(|asset| asset.content_type == "text/css; charset=utf-8"));
        assert_eq!(
            asset("/favicon.ico").expect("favicon").content_type,
            "image/x-icon"
        );
        assert_eq!(content_type("/favicon.ico"), "image/x-icon");
        assert_eq!(
            content_type("/assets/app.js"),
            "text/javascript; charset=utf-8"
        );
    }

    #[test]
    fn publishes_manifest_paths() {
        let manifest = asset_manifest();
        assert!(manifest.contains(&"/index.html"));
        assert!(manifest.contains(&"/favicon.ico"));
    }
}
