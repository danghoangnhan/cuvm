# cuvm installer (Windows) — download the latest release binary + shims and install them.
#
#   powershell -ExecutionPolicy ByPass -c "irm https://raw.githubusercontent.com/danghoangnhan/cuvm/main/install.ps1 | iex"
#
# Knobs (environment variables):
#   CUVM_VERSION         install a specific version, e.g. 0.1.0   (default: latest release)
#   CUVM_INSTALL_DIR     bin dir for cuvm.exe        (default: %USERPROFILE%\.cuvm\bin)
#   CUVM_HOME            cuvm data dir; shims land in <home>\shims  (default: %USERPROFILE%\.cuvm)
#   CUVM_NO_MODIFY_PATH  set to any value to skip adding the bin dir to your user PATH
#   CUVM_DOWNLOAD_BASE   release-asset base URL (default: GitHub releases; override for a mirror)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo     = 'danghoangnhan/cuvm'
$Releases = "https://github.com/$Repo/releases"
$DlBase   = if ($env:CUVM_DOWNLOAD_BASE) { $env:CUVM_DOWNLOAD_BASE } else { "$Releases/download" }

function Say($m) { Write-Host "cuvm: $m" }
function Die($m) { Write-Error "cuvm: error: $m"; exit 1 }

# --- detect architecture → release asset name --------------------------------
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne 'AMD64') {
  Die "no Windows cuvm binary for '$arch' (only windows-amd64 is built). See $Releases."
}
$name = 'windows-amd64'

# --- resolve version ---------------------------------------------------------
$ver = $env:CUVM_VERSION
if (-not $ver) {
  Say 'resolving the latest release...'
  try {
    $rel = Invoke-RestMethod -UseBasicParsing "https://api.github.com/repos/$Repo/releases/latest"
    $ver = $rel.tag_name
  } catch { Die "could not determine the latest release (set CUVM_VERSION to override): $_" }
}
$ver = $ver -replace '^v', ''   # accept "0.1.0" or "v0.1.0"

$stage   = "cuvm-$ver-$name"
$archive = "$stage.zip"
$url     = "$DlBase/v$ver/$archive"

# --- download into a scratch dir (cleaned at the end) ------------------------
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("cuvm-install-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp -Force | Out-Null
try {
  $zip = Join-Path $tmp $archive
  Say "downloading $archive..."
  try { Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $zip } catch { Die "download failed: $url" }

  # --- verify the checksum if SHA256SUMS is published ------------------------
  try {
    $sumsPath = Join-Path $tmp 'SHA256SUMS'
    Invoke-WebRequest -UseBasicParsing -Uri "$DlBase/v$ver/SHA256SUMS" -OutFile $sumsPath
    $want = (Select-String -Path $sumsPath -Pattern ([Regex]::Escape($archive)) | Select-Object -First 1).Line -split '\s+' | Select-Object -First 1
    if ($want) {
      $got = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
      if ($got -ne $want.ToLower()) { Die "checksum mismatch for $archive (expected $want, got $got)" }
      Say 'checksum OK'
    }
  } catch { Say 'SHA256SUMS not published for this release; skipping checksum verification' }

  # --- unpack + install ------------------------------------------------------
  Expand-Archive -Path $zip -DestinationPath $tmp -Force
  $srcExe = Join-Path $tmp "$stage\cuvm.exe"
  if (-not (Test-Path $srcExe)) { Die "archive did not contain $stage\cuvm.exe" }

  $binDir   = if ($env:CUVM_INSTALL_DIR) { $env:CUVM_INSTALL_DIR } else { Join-Path $env:USERPROFILE '.cuvm\bin' }
  $cuvmHome = if ($env:CUVM_HOME) { $env:CUVM_HOME } else { Join-Path $env:USERPROFILE '.cuvm' }
  $shimDir  = Join-Path $cuvmHome 'shims'
  New-Item -ItemType Directory -Path $binDir, $shimDir -Force | Out-Null
  Copy-Item -Path $srcExe -Destination (Join-Path $binDir 'cuvm.exe') -Force
  $srcShims = Join-Path $tmp "$stage\shims"
  if (Test-Path $srcShims) { Copy-Item -Path (Join-Path $srcShims '*') -Destination $shimDir -Force }

  Say "installed cuvm $ver -> $binDir\cuvm.exe"

  # --- add bin dir to the user PATH (idempotent) -----------------------------
  if (-not $env:CUVM_NO_MODIFY_PATH) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($userPath -split ';') -notcontains $binDir) {
      [Environment]::SetEnvironmentVariable('Path', "$binDir;$userPath", 'User')
      $env:Path = "$binDir;$env:Path"
      Say "added $binDir to your user PATH (restart your terminal to pick it up)"
    }
  }

  Write-Host ''
  Say 'next step — enable shell integration (cd-autoload + the cuvm wrapper):'
  Write-Host "  Add to your PowerShell `$PROFILE:"
  Write-Host "      . `"$shimDir\cuvm.ps1`""
  Write-Host ''
  Write-Host '  then restart PowerShell and run: cuvm --help'
}
finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
