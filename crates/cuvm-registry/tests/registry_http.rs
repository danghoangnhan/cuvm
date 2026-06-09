//! End-to-end registry tests against a local httpmock server — no live network.

use cuvm_app::RegistryClient;
use cuvm_core::{Arch, Os, Platform, Version};
use cuvm_registry::DefaultRegistryClient;
use httpmock::prelude::*;

fn linux() -> Platform {
    Platform {
        os: Os::Linux,
        arch: Arch::X86_64,
    }
}

const INDEX_HTML: &str = r#"<html><body>
<a href="redistrib_11.8.0.json">redistrib_11.8.0.json</a>
<a href="redistrib_12.4.1.json">redistrib_12.4.1.json</a>
<a href="redistrib_13.3.0.json">redistrib_13.3.0.json</a>
<a href="redistrib_12.4.1.json">dup link, must dedupe</a>
<a href="some_other_file.json">ignored</a>
</body></html>"#;

#[test]
fn list_toolkits_scrapes_and_sorts_versions() {
    let server = MockServer::start();
    let index = server.mock(|when, then| {
        when.method(GET).path("/redist/");
        then.status(200).body(INDEX_HTML);
    });

    let base = format!("{}/redist/", server.base_url());
    let client = DefaultRegistryClient::with_base_url(base);
    let versions = client.list_toolkits(&linux()).expect("list");

    index.assert();
    let raws: Vec<&str> = versions.iter().map(|v| v.raw.as_str()).collect();
    assert_eq!(raws, vec!["11.8.0", "12.4.1", "13.3.0"]);
    assert!(versions.contains(&Version::parse("12.4.1").unwrap()));
}

#[test]
fn list_toolkits_errors_on_empty_index() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/redist/");
        then.status(200)
            .body("<html><body>nothing here</body></html>");
    });
    let base = format!("{}/redist/", server.base_url());
    let client = DefaultRegistryClient::with_base_url(base);
    let err = client.list_toolkits(&linux()).unwrap_err();
    assert!(err.to_string().contains("no redistrib"));
}
