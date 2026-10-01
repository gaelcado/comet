//! Codex's packaged WebRTC helper protocol (Apache-2.0, OpenAI rust-v0.159.0).
//! Audio, AEC, interruption and encrypted media stay inside that native process.
//! Only bounded SDP signaling, controls and level meters cross this pipe.
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{ChildStdin, ChildStdout, Command},
};
use zeron_proto::voice::VoiceRejection;
const MAX_FRAME: usize = 128 * 1024;

pub(super) fn helper_path(executable: &Path) -> Result<PathBuf, VoiceRejection> {
    let binary = executable
        .canonicalize()
        .map_err(|_| VoiceRejection::NativeRuntimeUnavailable)?;
    let root = binary
        .parent()
        .and_then(Path::parent)
        .ok_or(VoiceRejection::NativeRuntimeUnavailable)?;
    let manifest: Value = std::fs::read(root.join("codex-package.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or(VoiceRejection::NativeRuntimeUnavailable)?;
    let version = manifest["version"]
        .as_str()
        .and_then(|v| semver::Version::parse(v).ok())
        .ok_or(VoiceRejection::Unsupported)?;
    if manifest["layoutVersion"] != 1 || version < semver::Version::new(0, 159, 0) {
        return Err(VoiceRejection::Unsupported);
    }
    let path = root
        .join("codex-resources/voice/bin")
        .join(if cfg!(windows) {
            "codex-voice-host.exe"
        } else {
            "codex-voice-host"
        });
    if path.canonicalize().ok().as_ref() != Some(&path) || !path.is_file() {
        return Err(VoiceRejection::NativeRuntimeUnavailable);
    }
    Ok(path)
}

pub(super) struct NativeHost {
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
        tokio::time::timeout(Duration::from_secs(seconds), async {
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
        .map_err(|_| VoiceRejection::Protocol)?
    }
    pub async fn expect(
        &mut self,
        message: Value,
        expected: &str,
        seconds: u64,
    ) -> Result<(), VoiceRejection> {
        if self.exchange(message, seconds).await?["type"] != expected {
            return Err(VoiceRejection::Protocol);
        }
        Ok(())
    }
    pub async fn controls(&mut self, muted: bool) -> Result<(), VoiceRejection> {
        self.expect(json!({"type":"setAudioControls","controls":{"microphoneMuted":muted,"speakerSuppressed":false}}),"audioControlsApplied",5).await
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
