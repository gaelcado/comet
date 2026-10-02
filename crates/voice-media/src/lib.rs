//! Codex's packaged WebRTC helper protocol (Apache-2.0, OpenAI rust-v0.159.0).
//! Audio, AEC, interruption and encrypted media stay inside that native process.
//! Only bounded SDP signaling, controls and level meters cross this pipe.
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{ChildStdin, ChildStdout, Command},
};
use zeron_proto::voice::VoiceRejection;
const MAX_FRAME: usize = 128 * 1024;

pub struct NativeHost {
    stop: tokio_util::sync::CancellationToken,
    input: ChildStdin,
    output: ChildStdout,
}
impl Drop for NativeHost {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
impl NativeHost {
    fn command(path: &Path) -> Command {
        let mut cmd = Command::new(path);
        // Fixed allowlist mirrors Codex. Never inherit API keys or dynamic loader/plugin overrides.
        cmd.env_clear();
        for (k, v) in std::env::vars_os() {
            if matches!(
                k.to_string_lossy().to_ascii_uppercase().as_str(),
                "SYSTEMROOT"
                    | "WINDIR"
                    | "HOME"
                    | "USERPROFILE"
                    | "LOCALAPPDATA"
                    | "APPDATA"
                    | "TEMP"
                    | "TMP"
                    | "TMPDIR"
                    | "XDG_RUNTIME_DIR"
                    | "PULSE_SERVER"
                    | "PULSE_COOKIE"
                    | "PIPEWIRE_REMOTE"
                    | "DBUS_SESSION_BUS_ADDRESS"
                    | "HTTP_PROXY"
                    | "HTTPS_PROXY"
                    | "ALL_PROXY"
                    | "NO_PROXY"
                    | "SSL_CERT_FILE"
                    | "SSL_CERT_DIR"
                    | "REQUESTS_CA_BUNDLE"
                    | "CURL_CA_BUNDLE"
            ) {
                cmd.env(k, v);
            }
        }
        for k in [
            "GST_PLUGIN_PATH",
            "GST_PLUGIN_PATH_1_0",
            "GST_PLUGIN_SYSTEM_PATH",
            "GST_PLUGIN_SYSTEM_PATH_1_0",
        ] {
            cmd.env(k, "");
        }
        cmd.env(
            "GST_REGISTRY",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GST_REGISTRY_UPDATE", "no")
        .env("GST_REGISTRY_FORK", "no");
        #[cfg(target_os = "linux")]
        for directory in [
            "/usr/lib/x86_64-linux-gnu/alsa-lib",
            "/usr/lib/aarch64-linux-gnu/alsa-lib",
            "/usr/lib64/alsa-lib",
            "/usr/lib/alsa-lib",
        ] {
            if Path::new(directory).is_dir() {
                cmd.env("ALSA_PLUGIN_DIR", directory);
                break;
            }
        }
        cmd.current_dir(path.parent().unwrap())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        cmd
    }
    pub async fn open(
        path: &Path,
        stop: tokio_util::sync::CancellationToken,
    ) -> Result<Self, VoiceRejection> {
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            Self::command(path).arg("--build-commit").output(),
        )
        .await
        .map_err(|_| VoiceRejection::NativeRuntimeUnavailable)?
        .map_err(|_| VoiceRejection::NativeRuntimeUnavailable)?;
        let commit = std::str::from_utf8(&result.stdout)
            .ok()
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .filter(|_| result.status.success())
            .ok_or(VoiceRejection::NativeRuntimeUnavailable)?;
        let mut child = Self::command(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|_| VoiceRejection::NativeRuntimeUnavailable)?;
        let input = child.stdin.take().ok_or(VoiceRejection::Protocol)?;
        let output = child.stdout.take().ok_or(VoiceRejection::Protocol)?;
        let reaper_stop = stop.clone();
        tokio::spawn(async move {
            tokio::select! {biased;
                _=reaper_stop.cancelled()=>{let _=child.start_kill();let _=child.wait().await;},
                _=child.wait()=>{},
            }
        });
        let mut host = Self {
            stop: stop.clone(),
            input,
            output,
        };
        host.expect(
            json!({"type":"hello","protocol":1,"buildCommit":commit}),
            "ready",
            30,
        )
        .await?;
        host.expect(json!({"type":"initializeRuntime"}), "runtimeReady", 30)
            .await?;
        Ok(host)
    }
    pub async fn exchange(
        &mut self,
        message: Value,
        seconds: u64,
    ) -> Result<Value, VoiceRejection> {
        if self.stop.is_cancelled() {
            return Err(VoiceRejection::Protocol);
        }
        struct ExchangeGuard(Option<tokio_util::sync::CancellationToken>);
        impl Drop for ExchangeGuard {
            fn drop(&mut self) {
                if let Some(stop) = &self.0 {
                    stop.cancel();
                }
            }
        }
        // A cancelled partial read invalidates the pipe, including when the
        // caller drops this future without dropping the media endpoint yet.
        let mut guard = ExchangeGuard(Some(self.stop.clone()));
        let result = tokio::time::timeout(Duration::from_secs(seconds), async {
            let payload = serde_json::to_vec(&message).map_err(|_| VoiceRejection::Protocol)?;
            if payload.len() > MAX_FRAME {
                return Err(VoiceRejection::Overflow);
            }
            self.input
                .write_u32(payload.len() as u32)
                .await
                .map_err(|_| VoiceRejection::Protocol)?;
            self.input
                .write_all(&payload)
                .await
                .map_err(|_| VoiceRejection::Protocol)?;
            self.input
                .flush()
                .await
                .map_err(|_| VoiceRejection::Protocol)?;
            let length = self
                .output
                .read_u32()
                .await
                .map_err(|_| VoiceRejection::Protocol)? as usize;
            if length == 0 || length > MAX_FRAME {
                return Err(VoiceRejection::Overflow);
            }
            let mut payload = vec![0; length];
            self.output
                .read_exact(&mut payload)
                .await
                .map_err(|_| VoiceRejection::Protocol)?;
            serde_json::from_slice(&payload).map_err(|_| VoiceRejection::Protocol)
        })
        .await
        .map_err(|_| VoiceRejection::Protocol)?;
        if result.is_ok() {
            guard.0 = None;
        }
        result
    }
    pub async fn expect(
        &mut self,
        message: Value,
        expected: &str,
        seconds: u64,
    ) -> Result<(), VoiceRejection> {
        if self.exchange(message, seconds).await?["type"] != expected {
            self.stop.cancel();
            return Err(VoiceRejection::Protocol);
        }
        Ok(())
    }
    pub async fn controls(&mut self, muted: bool) -> Result<(), VoiceRejection> {
        self.expect(json!({"type":"setAudioControls","controls":{"microphoneMuted":muted,"speakerSuppressed":false}}),"audioControlsApplied",5).await
    }
    /// Synchronous local shutdown, independent of pending RPC or helper I/O.
    pub fn close(&self) {
        self.stop.cancel();
    }
    pub async fn levels(&mut self) -> Result<(u16, u16), VoiceRejection> {
        let v = self.exchange(json!({"type":"inspectAudio"}), 5).await?;
        if v["type"] != "audioState" {
            return Err(VoiceRejection::Protocol);
        }
        let mic = v["state"]["microphonePeak"]
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .ok_or(VoiceRejection::Protocol)?;
        let speaker = v["state"]["speakerPeak"]
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .ok_or(VoiceRejection::Protocol)?;
        Ok((mic, speaker))
    }
}

/// An explicit development runtime or the signed application's resources. Never
/// searches PATH, resolves Codex, or reads its authentication directory.
pub fn bundled_helper() -> Result<std::path::PathBuf, VoiceRejection> {
    let root = if let Some(path) = std::env::var_os("ZERON_VOICE_MEDIA_DIR") {
        std::path::PathBuf::from(path)
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent()?.parent().map(|p| p.join("Resources/voice")))
            .ok_or(VoiceRejection::NativeRuntimeUnavailable)?
    };
    verify_runtime(&root)
}

pub const BUILD_COMMIT: &str = "a956835d020762cb2b570053af06f643a11c0ecc";

pub fn verify_runtime(root: &Path) -> Result<std::path::PathBuf, VoiceRejection> {
    use sha2::{Digest, Sha256};
    let reject = || VoiceRejection::NativeRuntimeUnavailable;
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(root.join("zeron-runtime.json")).map_err(|_| reject())?,
    )
    .map_err(|_| reject())?;
    if manifest["buildCommit"] != BUILD_COMMIT || manifest["protocol"] != 1 {
        return Err(VoiceRejection::Unsupported);
    }
    let files = manifest["sha256"].as_object().ok_or_else(reject)?;
    let helper = "bin/codex-voice-host";
    if !files.contains_key(helper)
        || !files.contains_key("runtime.json")
        || !files.contains_key("NOTICE.md")
    {
        return Err(reject());
    }
    for (name, digest) in files {
        let path = Path::new(name);
        if path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(reject());
        }
        let bytes = std::fs::read(root.join(path)).map_err(|_| reject())?;
        if digest.as_str() != Some(format!("{:x}", Sha256::digest(&bytes)).as_str()) {
            return Err(reject());
        }
    }
    Ok(root.join(helper))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn runtime_rejects_missing_tampered_and_incompatible_resources() {
        let dir = tempfile::tempdir().unwrap();
        assert!(verify_runtime(dir.path()).is_err());
        let mut hashes = serde_json::Map::new();
        for name in ["bin/codex-voice-host", "runtime.json", "NOTICE.md"] {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, name).unwrap();
            hashes.insert(
                name.into(),
                json!(format!("{:x}", Sha256::digest(name.as_bytes()))),
            );
        }
        let mut manifest = json!({"protocol":1,"buildCommit":BUILD_COMMIT,"sha256":hashes});
        let save = |m: &Value| {
            std::fs::write(
                dir.path().join("zeron-runtime.json"),
                serde_json::to_vec(m).unwrap(),
            )
            .unwrap()
        };
        save(&manifest);
        assert_eq!(
            verify_runtime(dir.path()).unwrap(),
            dir.path().join("bin/codex-voice-host")
        );
        std::fs::write(dir.path().join("runtime.json"), "tampered").unwrap();
        assert!(verify_runtime(dir.path()).is_err());
        std::fs::write(dir.path().join("runtime.json"), "runtime.json").unwrap();
        manifest["protocol"] = json!(2);
        save(&manifest);
        assert_eq!(
            verify_runtime(dir.path()).unwrap_err(),
            VoiceRejection::Unsupported
        );
        manifest["protocol"] = json!(1);
        manifest["sha256"]["../escape"] = json!("digest");
        save(&manifest);
        assert!(verify_runtime(dir.path()).is_err());
    }
}
