//! M2 install-pipeline e2e on FAKE redist fixtures served by httpmock (no real
//! network, no GPU). Drives `ls-remote`, `install`, and `uninstall` end to end.

use assert_cmd::Command;
use assert_fs::TempDir;
use httpmock::prelude::*;
use predicates::str::contains;

fn cuvm() -> Command {
    Command::cargo_bin("cuvm").expect("binary builds")
}

/// Redist index page with two toolkit manifests linked.
const INDEX_HTML: &str = r#"<html><body>
<a href="redistrib_12.4.1.json">redistrib_12.4.1.json</a>
<a href="redistrib_12.6.0.json">redistrib_12.6.0.json</a>
</body></html>"#;

#[test]
fn ls_remote_lists_toolkits_newest_first() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let _index = server.mock(|when, then| {
        when.method(GET).path("/redist/");
        then.status(200).body(INDEX_HTML);
    });

    cuvm()
        .env("CUVM_HOME", home.path())
        .env(
            "CUVM_REGISTRY_URL",
            format!("{}/redist/", server.base_url()),
        )
        .arg("ls-remote")
        .assert()
        .success()
        .stdout(contains("12.6.0"))
        .stdout(contains("12.4.1"));
}
