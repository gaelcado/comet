//! Legacy Codex package resolver. V2 media uses the Zeron bundle instead.
use serde_json::Value;
use std::path::{Path, PathBuf};
use zeron_proto::voice::VoiceRejection;
pub(super) use zeron_voice_media::NativeHost;

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
