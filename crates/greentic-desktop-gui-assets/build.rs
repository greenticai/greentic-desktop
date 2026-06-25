use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() -> io::Result<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let workspace_dir = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let dist_dir = workspace_dir.join("frontend/automate-hub/dist");
    let fallback_dir = manifest_dir.join("assets");
    let public_favicon = workspace_dir.join("frontend/automate-hub/public/favicon.ico");

    println!("cargo:rerun-if-changed={}", fallback_dir.display());
    println!("cargo:rerun-if-changed={}", public_favicon.display());
    println!("cargo:rerun-if-changed={}", dist_dir.display());

    let mut files = if dist_dir.join("index.html").is_file() {
        collect_files(&dist_dir)?
    } else {
        collect_files(&fallback_dir)?
    };

    if !files
        .iter()
        .any(|(_, route_path)| route_path == "/favicon.ico")
        && public_favicon.is_file()
    {
        files.push((public_favicon, "/favicon.ico".to_string()));
    }

    files.sort_by(|(_, left), (_, right)| left.cmp(right));

    let mut generated = String::from(
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub(crate) struct EmbeddedAsset {\n\
         \x20   pub path: &'static str,\n\
         \x20   pub content_type: &'static str,\n\
         \x20   pub bytes: &'static [u8],\n\
         \x20   pub etag: &'static str,\n\
         }\n\n\
         pub(crate) static EMBEDDED_ASSETS: &[EmbeddedAsset] = &[\n",
    );

    for (file_path, route_path) in &files {
        let bytes = fs::read(file_path)?;
        let etag = format!("w/greentic-gui-{:016x}-{}", fnv1a64(&bytes), bytes.len());
        generated.push_str("    EmbeddedAsset {\n");
        generated.push_str(&format!("        path: {:?},\n", route_path));
        generated.push_str(&format!(
            "        content_type: {:?},\n",
            content_type(route_path)
        ));
        generated.push_str(&format!(
            "        bytes: include_bytes!({:?}),\n",
            file_path.display().to_string()
        ));
        generated.push_str(&format!("        etag: {:?},\n", etag));
        generated.push_str("    },\n");
    }

    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    fs::write(out_dir.join("assets.rs"), generated)?;

    Ok(())
}

fn collect_files(root: &Path) -> io::Result<Vec<(PathBuf, String)>> {
    let mut files = Vec::new();
    collect_files_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_files_inner(
    root: &Path,
    dir: &Path,
    files: &mut Vec<(PathBuf, String)>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            collect_files_inner(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("file under root")
                .to_string_lossy()
                .replace('\\', "/");
            files.push((path, format!("/{relative}")));
        }
    }

    Ok(())
}

fn content_type(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
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

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
