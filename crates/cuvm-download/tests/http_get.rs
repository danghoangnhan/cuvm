use cuvm_download::{http_get, DownloadError};
use httpmock::prelude::*;

#[test]
fn http_get_returns_body_bytes_on_200() {
    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(GET).path("/redistrib_12.4.1.json");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"release_label":"12.4.1"}"#);
    });

    let url = server.url("/redistrib_12.4.1.json");
    let body = http_get(&url).expect("200 should yield bytes");
    m.assert();
    assert_eq!(body, br#"{"release_label":"12.4.1"}"#);
}

#[test]
fn http_get_maps_404_to_http_status() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/missing.json");
        then.status(404).body("nope");
    });

    let url = server.url("/missing.json");
    let err = http_get(&url).expect_err("404 should error");
    match err {
        DownloadError::HttpStatus { status, url: u } => {
            assert_eq!(status, 404);
            assert!(u.ends_with("/missing.json"), "{u}");
        }
        other => panic!("expected HttpStatus, got {other:?}"),
    }
}
