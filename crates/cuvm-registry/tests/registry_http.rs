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

use cuvm_app::ComponentPolicy;

const REDIST_133_BODY: &str = r#"{
  "release_label": "13.3.0",
  "cuda_nvcc": {
    "version": "13.3.33",
    "linux-x86_64": {
      "relative_path": "cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-13.3.33-archive.tar.xz",
      "sha256": "aaa111", "md5": "m1", "size": 100
    },
    "windows-x86_64": {
      "relative_path": "cuda_nvcc/windows-x86_64/cuda_nvcc-windows-x86_64-13.3.33-archive.zip",
      "sha256": "aaa222", "size": 200
    }
  },
  "cuda_cudart": {
    "version": "13.3.29",
    "linux-x86_64": {
      "relative_path": "cuda_cudart/linux-x86_64/cuda_cudart-linux-x86_64-13.3.29-archive.tar.xz",
      "sha256": "bbb111", "size": 80
    }
  },
  "cuda_crt": {
    "version": "13.3.33",
    "linux-x86_64": {
      "relative_path": "cuda_crt/linux-x86_64/cuda_crt-linux-x86_64-13.3.33-archive.tar.xz",
      "sha256": "ccc111", "size": 30
    }
  },
  "cccl": {
    "version": "13.3.3.3.1",
    "linux-x86_64": {
      "relative_path": "cccl/linux-x86_64/cccl-linux-x86_64-13.3.3.3.1-archive.tar.xz",
      "sha256": "ddd111", "size": 40
    }
  },
  "libnvvm": {
    "version": "13.3.33",
    "linux-x86_64": {
      "relative_path": "libnvvm/linux-x86_64/libnvvm-linux-x86_64-13.3.33-archive.tar.xz",
      "sha256": "eee111", "size": 60
    }
  },
  "cuda_nvrtc": {
    "version": "13.3.33",
    "linux-x86_64": {
      "relative_path": "cuda_nvrtc/linux-x86_64/cuda_nvrtc-linux-x86_64-13.3.33-archive.tar.xz",
      "sha256": "fff111", "size": 70
    }
  }
}"#;

#[test]
fn resolve_toolkit_recommended_emits_verbatim_urls() {
    let server = MockServer::start();
    let manifest = server.mock(|when, then| {
        when.method(GET).path("/redist/redistrib_13.3.0.json");
        then.status(200).body(REDIST_133_BODY);
    });

    let base = format!("{}/redist/", server.base_url());
    let client = DefaultRegistryClient::with_base_url(base.clone());
    let v = Version::parse("13.3.0").unwrap();
    let arts = client
        .resolve_toolkit(&v, &linux(), &ComponentPolicy::Recommended)
        .expect("resolve");
    manifest.assert();

    // 13.x recommended set (all present in the fixture).
    let comps: Vec<&str> = arts.iter().map(|a| a.component.as_str()).collect();
    assert_eq!(
        comps,
        vec![
            "cuda_nvcc",
            "cuda_cudart",
            "cuda_crt",
            "cccl",
            "libnvvm",
            "cuda_nvrtc"
        ]
    );

    let nvcc = arts.iter().find(|a| a.component == "cuda_nvcc").unwrap();
    let expected_url = format!(
        "{}redist/cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-13.3.33-archive.tar.xz",
        server.base_url() + "/"
    );
    assert_eq!(
        nvcc.url, expected_url,
        "url must be base + relative_path verbatim"
    );
    assert_eq!(
        nvcc.relative_path,
        "cuda_nvcc/linux-x86_64/cuda_nvcc-linux-x86_64-13.3.33-archive.tar.xz"
    );
    assert_eq!(nvcc.sha256, "aaa111");
    assert_eq!(nvcc.md5.as_deref(), Some("m1"));
    assert_eq!(nvcc.size, 100);
}

#[test]
fn resolve_toolkit_only_filters_to_allowlist() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/redist/redistrib_13.3.0.json");
        then.status(200).body(REDIST_133_BODY);
    });
    let base = format!("{}/redist/", server.base_url());
    let client = DefaultRegistryClient::with_base_url(base);
    let v = Version::parse("13.3.0").unwrap();
    let arts = client
        .resolve_toolkit(
            &v,
            &linux(),
            &ComponentPolicy::Only(vec!["cuda_nvcc".into(), "cuda_cudart".into()]),
        )
        .unwrap();
    let comps: Vec<&str> = arts.iter().map(|a| a.component.as_str()).collect();
    assert_eq!(comps, vec!["cuda_nvcc", "cuda_cudart"]);
}

#[test]
fn resolve_toolkit_errors_when_platform_missing() {
    // Request windows-x86_64; only cuda_nvcc has it, cuda_cudart does not → error.
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/redist/redistrib_13.3.0.json");
        then.status(200).body(REDIST_133_BODY);
    });
    let base = format!("{}/redist/", server.base_url());
    let client = DefaultRegistryClient::with_base_url(base);
    let v = Version::parse("13.3.0").unwrap();
    let win = Platform {
        os: Os::Windows,
        arch: Arch::X86_64,
    };
    let err = client
        .resolve_toolkit(
            &v,
            &win,
            &ComponentPolicy::Only(vec!["cuda_nvcc".into(), "cuda_cudart".into()]),
        )
        .unwrap_err();
    assert!(err.to_string().contains("windows-x86_64"));
}

// Descending order + a duplicate link, so the assertions below actually
// lock the sort and dedup behaviors (an already-sorted, dup-free index
// would pass even without them).
const CUDNN_INDEX_HTML: &str = r#"<html><body>
<a href="redistrib_9.8.0.json">redistrib_9.8.0.json</a>
<a href="redistrib_8.9.7.json">redistrib_8.9.7.json</a>
<a href="redistrib_8.9.7.json">dup link, must dedupe</a>
</body></html>"#;

const CUDNN_980_BODY: &str = r#"{
    "release_label": "9.8.0",
    "cudnn": {
        "name": "NVIDIA CUDA Deep Neural Network library",
        "license_path": "cudnn/LICENSE.txt",
        "version": "9.8.0.87",
        "linux-x86_64": {
            "cuda12": {
                "relative_path": "cudnn/linux-x86_64/cudnn-linux-x86_64-9.8.0.87_cuda12-archive.tar.xz",
                "sha256": "feed",
                "md5": "beef",
                "size": "1024"
            }
        }
    }
}"#;

#[test]
fn list_cudnn_scrapes_the_cudnn_index_sorted() {
    let server = MockServer::start();
    let cudnn_index = server.mock(|when, then| {
        when.method(GET).path("/cudnn/");
        then.status(200).body(CUDNN_INDEX_HTML);
    });
    let client = DefaultRegistryClient::with_base_urls(
        format!("{}/redist/", server.base_url()),
        format!("{}/cudnn/", server.base_url()),
        format!("{}/nccl/", server.base_url()),
    );
    let got = client.list_cudnn(&linux(), 12).expect("lists");
    let raws: Vec<&str> = got.iter().map(|v| v.raw.as_str()).collect();
    assert_eq!(raws, ["8.9.7", "9.8.0"]);
    cudnn_index.assert();
}

#[test]
fn resolve_cudnn_builds_the_artifact_from_the_cudnn_base() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/cudnn/redistrib_9.8.0.json");
        then.status(200).body(CUDNN_980_BODY);
    });
    let client = DefaultRegistryClient::with_base_urls(
        format!("{}/redist/", server.base_url()),
        format!("{}/cudnn/", server.base_url()),
        format!("{}/nccl/", server.base_url()),
    );
    let v = Version::parse("9.8.0").unwrap();
    let arts = client.resolve_cudnn(&v, &linux(), 12).expect("resolves");
    assert_eq!(arts.len(), 1);
    assert_eq!(arts[0].component, "cudnn");
    assert_eq!(arts[0].sha256, "feed");
    assert_eq!(arts[0].md5.as_deref(), Some("beef"));
    assert_eq!(arts[0].size, 1024);
    assert_eq!(
        arts[0].relative_path,
        "cudnn/linux-x86_64/cudnn-linux-x86_64-9.8.0.87_cuda12-archive.tar.xz"
    );
    assert_eq!(
        arts[0].url,
        format!(
            "{}/cudnn/cudnn/linux-x86_64/cudnn-linux-x86_64-9.8.0.87_cuda12-archive.tar.xz",
            server.base_url()
        )
    );
}

#[test]
fn resolve_cudnn_missing_variant_is_a_clear_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/cudnn/redistrib_9.8.0.json");
        then.status(200).body(CUDNN_980_BODY);
    });
    let client = DefaultRegistryClient::with_base_urls(
        format!("{}/redist/", server.base_url()),
        format!("{}/cudnn/", server.base_url()),
        format!("{}/nccl/", server.base_url()),
    );
    let v = Version::parse("9.8.0").unwrap();
    let err = client.resolve_cudnn(&v, &linux(), 13).unwrap_err();
    assert!(err.to_string().contains("cuda13"), "{err}");
}

#[test]
fn resolve_cudnn_404_error_names_the_status_and_manifest_url() {
    // No mock is registered for redistrib_9.9.9.json, so httpmock answers 404.
    // The error string must carry both the status and the manifest file name:
    // the CLI pairing warning surfaces this message verbatim (plan D5).
    let server = MockServer::start();
    let client = DefaultRegistryClient::with_base_urls(
        format!("{}/redist/", server.base_url()),
        format!("{}/cudnn/", server.base_url()),
        format!("{}/nccl/", server.base_url()),
    );
    let v = Version::parse("9.9.9").unwrap();
    let err = client.resolve_cudnn(&v, &linux(), 12).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("404"), "{msg}");
    assert!(msg.contains("redistrib_9.9.9.json"), "{msg}");
}

// ---- NCCL (spec §2.3: directory index, no manifest, no checksums) ----------

const NCCL_INDEX_HTML: &str = r"<html><body>
    <a href='..'>..</a>
    <a href='New folder/'>New folder/</a>
    <a href='v2.20.5/'>v2.20.5/</a>
    <a href='v2.21.5/'>v2.21.5/</a>
    <a href='v2.21.5/'>dup link</a>
</body></html>";

const NCCL_DIR_2215_HTML: &str = r"<html><body>
    <a href='nccl_2.21.5-1+cuda11.0_x86_64.txz'>nccl_2.21.5-1+cuda11.0_x86_64.txz</a>
    <a href='nccl_2.21.5-1+cuda12.2_x86_64.txz'>nccl_2.21.5-1+cuda12.2_x86_64.txz</a>
    <a href='nccl_2.21.5-1+cuda12.4_x86_64.txz'>nccl_2.21.5-1+cuda12.4_x86_64.txz</a>
    <a href='nccl_2.21.5-1+cuda12.4_aarch64.txz'>nccl_2.21.5-1+cuda12.4_aarch64.txz</a>
</body></html>";

fn nccl_client(server: &MockServer) -> DefaultRegistryClient {
    DefaultRegistryClient::with_base_urls(
        format!("{}/redist/", server.base_url()),
        format!("{}/cudnn/", server.base_url()),
        format!("{}/nccl/", server.base_url()),
    )
}

#[test]
fn list_nccl_scrapes_version_dirs_sorted_and_deduped() {
    let server = MockServer::start();
    let index = server.mock(|when, then| {
        when.method(GET).path("/nccl/");
        then.status(200).body(NCCL_INDEX_HTML);
    });
    let got = nccl_client(&server).list_nccl(&linux()).expect("lists");
    index.assert();
    let raws: Vec<&str> = got.iter().map(|v| v.raw.as_str()).collect();
    assert_eq!(raws, ["2.20.5", "2.21.5"]);
}

#[test]
fn resolve_nccl_picks_newest_cuda_minor_for_the_major_with_empty_sha() {
    let server = MockServer::start();
    let dir = server.mock(|when, then| {
        when.method(GET).path("/nccl/v2.21.5/");
        then.status(200).body(NCCL_DIR_2215_HTML);
    });
    let v = Version::parse("2.21.5").unwrap();
    // cuda12 → newest cuda12.* (12.4 over 12.2), x86_64 over the aarch64 sibling.
    let arts = nccl_client(&server)
        .resolve_nccl(&v, &linux(), 12)
        .expect("resolves");
    dir.assert();
    assert_eq!(arts.len(), 1);
    assert_eq!(arts[0].component, "nccl");
    assert_eq!(
        arts[0].relative_path,
        "v2.21.5/nccl_2.21.5-1+cuda12.4_x86_64.txz"
    );
    assert_eq!(
        arts[0].url,
        format!(
            "{}/nccl/v2.21.5/nccl_2.21.5-1+cuda12.4_x86_64.txz",
            server.base_url()
        )
    );
    // The NCCL redist ships no checksums — the artifact sha is empty (self-recorded).
    assert!(arts[0].sha256.is_empty(), "nccl sha must be self-recorded");
}

#[test]
fn resolve_nccl_no_build_for_cuda_major_is_a_clear_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/nccl/v2.21.5/");
        then.status(200).body(NCCL_DIR_2215_HTML);
    });
    let v = Version::parse("2.21.5").unwrap();
    let err = nccl_client(&server)
        .resolve_nccl(&v, &linux(), 13)
        .unwrap_err();
    assert!(err.to_string().contains("cuda13"), "{err}");
}
