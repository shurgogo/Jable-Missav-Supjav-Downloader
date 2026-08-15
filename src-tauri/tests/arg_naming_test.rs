//! Regression test: Tauri commands receive frontend args as a JSON object.
//! Tauri only camelCases the *top-level* argument keys; nested structs are
//! deserialized with plain serde, so a struct field like `save_dir` must be
//! reachable under the exact key the frontend sends (`saveDir`).
//!
//! This used to be broken: `DownloadRequest` had no `rename_all`, so
//! `start_download` always failed with "missing field `save_dir`", which made
//! the "download selected" button silently do nothing (same as "add to queue").

use avdl_lib::commands::downloader::DownloadRequest;

#[test]
fn download_request_accepts_camel_case_keys_as_sent_by_frontend() {
    let camel = serde_json::from_str::<DownloadRequest>(
        r#"{"site":"jable","url":"https://jable.tv/videos/abc-123/","saveDir":"download","maxConcurrent":3,"resolution":"highest"}"#,
    );
    assert!(
        camel.is_ok(),
        "frontend sends saveDir/maxConcurrent; struct must accept them: {:?}",
        camel
    );
    let req = camel.unwrap();
    assert_eq!(req.save_dir, "download");
    assert_eq!(req.max_concurrent, 3);
    assert_eq!(req.resolution, "highest");
}

#[test]
fn download_request_rejects_snake_case_keys() {
    // rename_all = "camelCase" means the wire format is camelCase; snake_case
    // payloads are not valid. Kept as documentation of the expected shape.
    let snake = serde_json::from_str::<DownloadRequest>(
        r#"{"site":"jable","url":"https://jable.tv/videos/abc-123/","save_dir":"download","max_concurrent":3,"resolution":"highest"}"#,
    );
    assert!(snake.is_err());
}
