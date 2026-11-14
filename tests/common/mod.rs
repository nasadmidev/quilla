use axum::Router;
use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tower_http::services::ServeFile;
use zstd::stream::Encoder;

const PACKAGE_NAME: &str = "pkg_test";
const ARCHIVE_NAME: &str = "pkg_test-1.0.0-any.pkg.tar.zst";

fn create_dummy_compressed(root: &Path) -> Result<(PathBuf, Vec<u8>), io::Error> {
    let compressed_path = root.join(ARCHIVE_NAME);
    let pkg_folder = root.join("package-root");
    let pkg_folder_bin = pkg_folder.join("usr/bin");
    fs::create_dir_all(&pkg_folder_bin)?;

    let binary_path = pkg_folder_bin.join("test");
    let mut binary = File::create(&binary_path)?;
    binary.write_all(b"#!/bin/sh\necho 'Hello Quilla'\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&binary_path, fs::Permissions::from_mode(0o755))?;
    }

    let compressed = File::create(&compressed_path)?;
    let encoder = Encoder::new(compressed, 1)?;
    let mut tar = tar::Builder::new(encoder);
    tar.append_dir_all(".", &pkg_folder)?;
    let encoder = tar.into_inner()?;
    encoder.finish()?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut compressed = File::open(&compressed_path)?;
    loop {
        let count = compressed.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok((compressed_path, hasher.finalize().to_vec()))
}

fn create_dummy_index(root: &Path, hash: &[u8]) -> Result<PathBuf, io::Error> {
    let hash = hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let desc = format!(
        "%FILENAME%\n{ARCHIVE_NAME}\n\
         %NAME%\n{PACKAGE_NAME}\n\
         %BASE%\n{PACKAGE_NAME}\n\
         %VERSION%\n1.0.0\n\
         %DESC%\nA dummy package\n\
         %CSIZE%\n108274\n\
         %ISIZE%\n1617309\n\
         %SHA256SUM%\n{hash}\n\
         %URL%\nhttps://example.invalid/\n\
         %LICENSE%\nLicenseRef-None\n\
         %ARCH%\nany\n"
    );

    let package_index_dir = root.join(format!("index/{PACKAGE_NAME}"));
    fs::create_dir_all(&package_index_dir)?;
    fs::write(package_index_dir.join("desc"), desc)?;

    let index_path = root.join("core.db.tar.gz");
    let compressed_index = File::create(&index_path)?;
    let encoder = GzEncoder::new(compressed_index, Compression::default());
    let mut tar = tar::Builder::new(encoder);
    tar.append_dir_all(".", &package_index_dir)?;
    let encoder = tar.into_inner()?;
    encoder.finish()?;

    Ok(index_path)
}

pub fn make_checksum_invalid(root: &Path) {
    let wrong_hash = [0u8; 32];
    create_dummy_index(root, &wrong_hash).expect("could not write checksum failure index");
}

pub fn replace_with_invalid_archive(root: &Path) {
    let archive_path = root.join(ARCHIVE_NAME);
    let invalid_archive = b"this is not a zstd-compressed tar archive";
    fs::write(&archive_path, invalid_archive).expect("could not write invalid package archive");
    let hash = Sha256::digest(invalid_archive);
    create_dummy_index(root, &hash).expect("could not write invalid archive index");
}

pub async fn run_server(root: &Path) -> String {
    let (package_path, hash) =
        create_dummy_compressed(root).expect("could not create compressed test package");
    let index_path =
        create_dummy_index(root, &hash).expect("could not create test repository index");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("could not bind local test server");
    let address: SocketAddr = listener
        .local_addr()
        .expect("could not determine local test server address");

    let app = Router::new()
        .route_service("/core/os/x86_64/core.db", ServeFile::new(index_path))
        .route_service(
            &format!("/core/os/x86_64/{ARCHIVE_NAME}"),
            ServeFile::new(package_path),
        );
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("local test server failed");
    });

    format!("http://{address}")
}
