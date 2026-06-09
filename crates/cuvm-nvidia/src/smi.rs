//! `nvidia-smi` driver probe (spec §2.4/§11).
//!
//! Read-only. Shells out to `nvidia-smi`; on a missing binary or non-zero exit
//! it returns a *driver-unknown* [`Driver`] (`present: false`) rather than an
//! error — "missing nvidia-smi -> driver unknown, build-only OK" (spec §11).

use std::process::Command;

use cuvm_app::DriverProbe;
use cuvm_core::domain::{Arch, Os, Platform};
use cuvm_core::version::Version;
use cuvm_core::{Driver, GpuClass};

/// Probe implementing the `DriverProbe` port. `binary` is overridable for tests.
pub struct SmiProbe {
    binary: String,
}

impl Default for SmiProbe {
    fn default() -> Self {
        SmiProbe {
            binary: "nvidia-smi".to_string(),
        }
    }
}

impl SmiProbe {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the binary path/name (used by tests with a fake script).
    #[must_use]
    pub fn with_binary(binary: impl Into<String>) -> Self {
        SmiProbe {
            binary: binary.into(),
        }
    }

    /// Probe the driver. Never errors on an absent `nvidia-smi`.
    ///
    /// # Errors
    /// Only returns an error for truly unexpected failures unrelated to
    /// `nvidia-smi` absence (e.g. internal parse errors that should never
    /// occur with well-formed output). Absent or non-zero-exit `nvidia-smi`
    /// yields `Driver { present: false }` without an error.
    pub fn probe_driver(&self) -> anyhow::Result<Driver> {
        let plat = host_platform();
        let output = Command::new(&self.binary)
            .args(["--query-gpu=driver_version,name", "--format=csv,noheader"])
            .output();

        let stdout = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            // Non-zero exit or spawn failure (NotFound / permission / etc.) -> driver unknown.
            Ok(_) | Err(_) => return Ok(driver_unknown(plat)),
        };

        match parse_smi_csv(&stdout) {
            Ok((version, gpu_class)) => Ok(Driver {
                present: true,
                version,
                platform: plat,
                gpu_class,
            }),
            // Unparseable output is treated as unknown, not a hard failure.
            Err(_) => Ok(driver_unknown(plat)),
        }
    }
}

/// A "driver unknown" record: present=false, version 0, class Unknown.
fn driver_unknown(platform: Platform) -> Driver {
    Driver {
        present: false,
        version: Version::parse("0").expect("0 parses"),
        platform,
        gpu_class: GpuClass::Unknown,
    }
}

/// Host platform for the probe result. Arch detection beyond `x86_64` is out of
/// scope here (spec gates arm64 behind its own integration run); default mirror.
fn host_platform() -> Platform {
    let os = if cfg!(windows) {
        Os::Windows
    } else {
        Os::Linux
    };
    let arch = if cfg!(target_arch = "aarch64") {
        Arch::Aarch64
    } else {
        Arch::X86_64
    };
    Platform { os, arch }
}

impl DriverProbe for SmiProbe {
    fn probe(&self) -> anyhow::Result<Driver> {
        self.probe_driver()
    }
}

/// Pure parser for one `nvidia-smi` CSV row: `<driver_version>, <gpu name>`.
/// Returns the first GPU's driver version and inferred class.
///
/// # Errors
/// Returns an error if the output is empty or cannot be parsed as
/// `driver_version, gpu_name`.
pub fn parse_smi_csv(s: &str) -> anyhow::Result<(Version, GpuClass)> {
    let line = s
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .ok_or_else(|| anyhow::anyhow!("nvidia-smi produced no GPU rows"))?;

    let mut parts = line.splitn(2, ',');
    let ver_str = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing driver_version field"))?;
    let name = parts.next().map_or("", str::trim);

    let version = Version::parse(ver_str)?;
    let gpu_class = classify_gpu(name);
    Ok((version, gpu_class))
}

/// Best-effort GPU-class inference from the marketing name. Conservative:
/// anything unrecognized is `Unknown` (so cuda-compat is never suggested for it).
fn classify_gpu(name: &str) -> GpuClass {
    let n = name.to_ascii_lowercase();
    if n.contains("geforce") || n.contains("titan") {
        GpuClass::GeForce
    } else if n.contains("jetson")
        || n.contains("orin")
        || n.contains("xavier")
        || n.contains("tegra")
    {
        GpuClass::Jetson
    } else if n.contains("a100")
        || n.contains("h100")
        || n.contains("h200")
        || n.contains("b200")
        || n.contains("a30")
        || n.contains("a40")
        || n.contains("l40")
        || n.contains("tesla")
        || n.contains("-sxm")
        || n.contains("nvidia a")
        || n.contains("nvidia h")
    {
        GpuClass::DataCenter
    } else {
        GpuClass::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuvm_core::version::Version;
    use cuvm_core::GpuClass;

    #[test]
    fn parses_driver_version_and_geforce_class() {
        let csv = "550.54.14, NVIDIA GeForce RTX 4090\n";
        let (ver, class) = parse_smi_csv(csv).unwrap();
        assert_eq!(ver, Version::parse("550.54.14").unwrap());
        assert_eq!(class, GpuClass::GeForce);
    }

    #[test]
    fn parses_datacenter_class() {
        let csv = "535.54.03, NVIDIA A100-SXM4-80GB\n";
        let (_ver, class) = parse_smi_csv(csv).unwrap();
        assert_eq!(class, GpuClass::DataCenter);
    }

    #[test]
    fn parses_jetson_class() {
        let csv = "540.00.00, Orin (nvgpu)\n";
        let (_ver, class) = parse_smi_csv(csv).unwrap();
        assert_eq!(class, GpuClass::Jetson);
    }

    #[test]
    fn driver_version_parsed_as_numeric_tuple_not_lexical() {
        // 570.124.06 must compare > 570.26 numerically (the §2.4 trap).
        let (a, _) = parse_smi_csv("570.124.06, NVIDIA H100\n").unwrap();
        let (b, _) = parse_smi_csv("570.26, NVIDIA H100\n").unwrap();
        assert!(a > b);
    }

    #[test]
    fn empty_output_is_an_error_not_a_panic() {
        assert!(parse_smi_csv("\n").is_err());
        assert!(parse_smi_csv("").is_err());
    }

    #[test]
    fn probe_returns_driver_unknown_when_smi_missing() {
        // Point at a binary that does not exist -> graceful absent, never a crash.
        let probe = SmiProbe::with_binary("definitely-not-nvidia-smi-xyz");
        let d = probe
            .probe_driver()
            .expect("probe must not error when smi absent");
        assert!(!d.present, "absent nvidia-smi must yield present=false");
        assert_eq!(d.gpu_class, GpuClass::Unknown);
    }

    #[cfg(unix)]
    #[test]
    fn probe_parses_a_fake_nvidia_smi_script() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("nvidia-smi");
        {
            // Drop the file handle before executing the script; an open write FD
            // causes ETXTBSY ("Text file busy") on Linux when exec'ing the file.
            let mut f = std::fs::File::create(&fake).unwrap();
            // Ignores args; prints one GPU row in the queried CSV shape.
            writeln!(f, "#!/bin/sh\necho '550.54.14, NVIDIA GeForce RTX 4090'").unwrap();
        }
        let mut perms = std::fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake, perms).unwrap();

        let probe = SmiProbe::with_binary(fake.to_str().unwrap());
        let d = probe.probe_driver().unwrap();
        assert!(d.present);
        assert_eq!(d.version, Version::parse("550.54.14").unwrap());
        assert_eq!(d.gpu_class, GpuClass::GeForce);
    }
}
