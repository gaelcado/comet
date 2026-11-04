//! Immutable widget content on disk:
//! `<store_root>/project-widgets/assets/<bundleId>/{manifest.json, script.luau,
//! assets/<name>}`.
//!
//! A bundle is written into an adjacent `.staging-*` directory, its files
//! synced and read back against their hashes, its script checked with the
//! visual runtime, and only then renamed into place. Nothing ever writes into
//! a published bundle; a widget points at one only after its transaction
//! commits, so no reader sees a bundle incomplete.

use std::collections::{BTreeMap, HashSet};
use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest, Sha256};
use zeron_proto::{
    PROJECT_WIDGET_RENDERER_LUAU_V1, ProjectWidgetAsset, ProjectWidgetContent,
    VISUAL_MAX_ASSET_BYTES, VISUAL_MAX_ASSETS, VISUAL_MAX_BYTES, VISUAL_MAX_TOTAL_ASSET_BYTES,
    valid_visual_asset_name,
};

use super::WidgetsError;

const MANIFEST: &str = "manifest.json";
const SCRIPT: &str = "script.luau";
const ASSETS: &str = "assets";
const STAGING_PREFIX: &str = ".staging-";
const FORMAT_VERSION: u32 = 1;
/// A multiple of 3, so base64 chunks concatenate losslessly
/// (`ReadAttachmentChunk`'s size).
pub(super) const READ_CHUNK_BYTES: u64 = 45_000;

/// Which file of a bundle to read.
pub(super) enum BundleFile<'a> {
    Script,
    Asset(&'a str),
}

pub(super) struct Bundles {
    root: PathBuf,
}

impl Bundles {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Validate, write, check and publish one bundle. On any failure nothing
    /// is left behind and no published bundle changes.
    pub(super) fn publish(
        &self,
        script: &str,
        assets: &BTreeMap<String, Vec<u8>>,
        seed: u64,
    ) -> Result<ProjectWidgetContent, WidgetsError> {
        validate(script, assets)?;
        let preview = super::preview::extract(script, seed, visual_assets(assets));
        let bundle_id = uuid::Uuid::new_v4().simple().to_string();
        let content = ProjectWidgetContent {
            bundle_id: bundle_id.clone(),
            format_version: FORMAT_VERSION,
            renderer_format: PROJECT_WIDGET_RENDERER_LUAU_V1.into(),
            render_seed: seed,
            script_size: script.len() as u64,
            script_sha256: sha256(script.as_bytes()),
            assets: assets
                .iter()
                .map(|(name, bytes)| ProjectWidgetAsset {
                    name: name.clone(),
                    size: bytes.len() as u64,
                    sha256: sha256(bytes),
                })
                .collect(),
            preview,
        };
        std::fs::create_dir_all(&self.root).map_err(write_error)?;
        let staging = self
            .root
            .join(format!("{STAGING_PREFIX}{}", uuid::Uuid::new_v4().simple()));
        let staged = (|| {
            std::fs::create_dir(&staging).map_err(write_error)?;
            write_synced(&staging.join(SCRIPT), script.as_bytes())?;
            if !assets.is_empty() {
                std::fs::create_dir(staging.join(ASSETS)).map_err(write_error)?;
            }
            for (name, bytes) in assets {
                write_synced(&staging.join(ASSETS).join(name), bytes)?;
            }
            let manifest = serde_json::to_vec_pretty(&content)
                .map_err(|err| WidgetsError::Write(err.to_string()))?;
            write_synced(&staging.join(MANIFEST), &manifest)?;
            // What landed on disk is what will be served.
            verify(&staging, &content)?;
            check(script, assets, seed)?;
            std::fs::rename(&staging, self.root.join(&bundle_id)).map_err(write_error)?;
            sync_dir(&self.root);
            Ok(())
        })();
        if let Err(err) = staged {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(err);
        }
        Ok(content)
    }

    /// Remove a bundle no widget points at (a transaction that failed after
    /// publishing it). Best effort; the collector finds it otherwise.
    pub(super) fn discard(&self, bundle_id: &str) {
        if valid_bundle_id(bundle_id) {
            let _ = std::fs::remove_dir_all(self.root.join(bundle_id));
        }
    }

    /// Every asset of a published bundle, verified against its manifest.
    pub(super) fn read_assets(
        &self,
        content: &ProjectWidgetContent,
    ) -> Result<BTreeMap<String, Vec<u8>>, WidgetsError> {
        let mut out = BTreeMap::new();
        for asset in &content.assets {
            let path = self.file(&content.bundle_id, BundleFile::Asset(&asset.name))?;
            let bytes = std::fs::read(&path).map_err(|_| WidgetsError::Conflict)?;
            if bytes.len() as u64 != asset.size || sha256(&bytes) != asset.sha256 {
                return Err(WidgetsError::Write(format!(
                    "stored asset '{}' does not match its manifest",
                    asset.name
                )));
            }
            out.insert(asset.name.clone(), bytes);
        }
        Ok(out)
    }

    /// A published bundle's script, verified against its manifest.
    pub(super) fn read_script(
        &self,
        content: &ProjectWidgetContent,
    ) -> Result<String, WidgetsError> {
        let path = self.file(&content.bundle_id, BundleFile::Script)?;
        let bytes = std::fs::read(&path).map_err(|_| WidgetsError::Conflict)?;
        if bytes.len() as u64 != content.script_size || sha256(&bytes) != content.script_sha256 {
            return Err(WidgetsError::Write(
                "stored script does not match its manifest".into(),
            ));
        }
        String::from_utf8(bytes).map_err(|err| WidgetsError::Write(err.to_string()))
    }

    /// One chunk of a published bundle's file of `size` bytes.
    pub(super) fn read_chunk(
        &self,
        bundle_id: &str,
        file: BundleFile<'_>,
        offset: u64,
        size: u64,
    ) -> Result<(String, u64), WidgetsError> {
        let path = self.file(bundle_id, file)?;
        // A missing file means the content was retired meanwhile.
        let mut handle = std::fs::File::open(&path).map_err(|_| WidgetsError::Conflict)?;
        let start = offset.min(size);
        let next = (start + READ_CHUNK_BYTES).min(size);
        let mut buf = vec![0u8; (next - start) as usize];
        handle
            .seek(std::io::SeekFrom::Start(start))
            .and_then(|_| handle.read_exact(&mut buf))
            .map_err(|_| WidgetsError::Conflict)?;
        Ok((BASE64.encode(&buf), next))
    }

    /// Remove unreferenced bundles and abandoned staging directories older
    /// than `grace` (a bundle is published just before the transaction that
    /// references it). Never touches anything outside the bundles root.
    pub(super) fn collect(&self, referenced: &HashSet<String>, grace: Duration) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let ours = name.starts_with(STAGING_PREFIX)
                || (valid_bundle_id(&name) && !referenced.contains(&name));
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            let old = meta
                .modified()
                .ok()
                .and_then(|at| at.elapsed().ok())
                .is_some_and(|age| age >= grace);
            if !ours || !old {
                continue;
            }
            let gone = if meta.is_dir() {
                std::fs::remove_dir_all(entry.path())
            } else {
                std::fs::remove_file(entry.path())
            };
            if gone.is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// The path of a published bundle's file, refusing anything that is not
    /// a plain file inside the bundles root (traversal, symlinks).
    fn file(&self, bundle_id: &str, file: BundleFile<'_>) -> Result<PathBuf, WidgetsError> {
        if !valid_bundle_id(bundle_id) {
            return Err(WidgetsError::NotFound);
        }
        let bundle = self.root.join(bundle_id);
        let path = match file {
            BundleFile::Script => bundle.join(SCRIPT),
            BundleFile::Asset(name) if valid_visual_asset_name(name) => {
                bundle.join(ASSETS).join(name)
            }
            BundleFile::Asset(_) => return Err(WidgetsError::NotFound),
        };
        let mut walked = self.root.clone();
        for part in path.strip_prefix(&self.root).unwrap_or(&path).components() {
            walked.push(part);
            let meta = walked
                .symlink_metadata()
                .map_err(|_| WidgetsError::Conflict)?;
            if meta.file_type().is_symlink() {
                return Err(WidgetsError::NotFound);
            }
        }
        if !path.is_file() {
            return Err(WidgetsError::NotFound);
        }
        let inside = std::fs::canonicalize(&self.root)
            .and_then(|root| Ok(std::fs::canonicalize(&path)?.starts_with(root)))
            .unwrap_or(false);
        if !inside {
            return Err(WidgetsError::NotFound);
        }
        Ok(path)
    }
}

/// The publication bounds of `PublishVisual`, on the bundle as a whole.
fn validate(script: &str, assets: &BTreeMap<String, Vec<u8>>) -> Result<(), WidgetsError> {
    let invalid = |message: String| Err(WidgetsError::InvalidVisual(message));
    if script.trim().is_empty() || script.len() > VISUAL_MAX_BYTES {
        return invalid(format!("script must be 1 to {VISUAL_MAX_BYTES} bytes"));
    }
    if assets.len() > VISUAL_MAX_ASSETS {
        return invalid(format!("a visual takes at most {VISUAL_MAX_ASSETS} assets"));
    }
    let mut total = 0;
    let mut seen = HashSet::new();
    for (name, bytes) in assets {
        if !valid_visual_asset_name(name) {
            return invalid(format!("asset name '{name}' must be a plain file name"));
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return invalid(format!(
                "asset names must differ by more than case ('{name}')"
            ));
        }
        total += bytes.len();
        if bytes.len() > VISUAL_MAX_ASSET_BYTES || total > VISUAL_MAX_TOTAL_ASSET_BYTES {
            return invalid(format!(
                "asset '{name}' is too large ({} MB each, {} MB together)",
                VISUAL_MAX_ASSET_BYTES / (1024 * 1024),
                VISUAL_MAX_TOTAL_ASSET_BYTES / (1024 * 1024)
            ));
        }
    }
    Ok(())
}

/// Run the script the way the desktop will, with the recorded seed.
fn check(script: &str, assets: &BTreeMap<String, Vec<u8>>, seed: u64) -> Result<(), WidgetsError> {
    zeron_visual::check_with_assets(script, seed, visual_assets(assets))
        .map(drop)
        .map_err(|err| WidgetsError::InvalidVisual(format!("visual script failed: {err}")))
}

pub(super) fn visual_assets(assets: &BTreeMap<String, Vec<u8>>) -> zeron_visual::Assets {
    zeron_visual::Assets::new(
        assets
            .iter()
            .map(|(name, bytes)| (name.clone(), bytes.as_slice().into()))
            .collect(),
    )
}

fn verify(dir: &Path, content: &ProjectWidgetContent) -> Result<(), WidgetsError> {
    let matches = |path: PathBuf, size: u64, hash: &str| {
        std::fs::read(path).is_ok_and(|bytes| bytes.len() as u64 == size && sha256(&bytes) == hash)
    };
    let script_ok = matches(
        dir.join(SCRIPT),
        content.script_size,
        &content.script_sha256,
    );
    let assets_ok = content.assets.iter().all(|asset| {
        matches(
            dir.join(ASSETS).join(&asset.name),
            asset.size,
            &asset.sha256,
        )
    });
    if script_ok && assets_ok {
        Ok(())
    } else {
        Err(WidgetsError::Write(
            "a widget file did not read back as written".into(),
        ))
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), WidgetsError> {
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(write_error)?;
    file.write_all(bytes).map_err(write_error)?;
    file.sync_all().map_err(write_error)
}

/// Make the rename durable. Best effort: not every platform syncs directories.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(dir) = std::fs::File::open(dir) {
        let _ = dir.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

fn write_error(err: std::io::Error) -> WidgetsError {
    WidgetsError::Write(err.to_string())
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Bundle ids are minted here as simple UUIDs; anything else is not ours.
fn valid_bundle_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT_OK: &str = "return function() return ui.text('hi') end";

    #[test]
    fn reads_refuse_traversal_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let bundles = Bundles::new(dir.path().join("assets"));
        let mut assets = BTreeMap::new();
        assets.insert("data.json".to_owned(), b"[1,2,3]".to_vec());
        let content = bundles.publish(SCRIPT_OK, &assets, 7).unwrap();
        let id = &content.bundle_id;
        assert_eq!(
            bundles
                .read_chunk(id, BundleFile::Asset("data.json"), 0, 7)
                .unwrap(),
            (BASE64.encode(b"[1,2,3]"), 7)
        );
        for name in ["../manifest.json", "..", "a/b", ""] {
            assert!(matches!(
                bundles.read_chunk(id, BundleFile::Asset(name), 0, 1),
                Err(WidgetsError::NotFound)
            ));
        }
        assert!(
            bundles
                .read_chunk("../x", BundleFile::Script, 0, 1)
                .is_err()
        );
        #[cfg(unix)]
        {
            let outside = dir.path().join("secret");
            std::fs::write(&outside, b"secret").unwrap();
            let link = dir
                .path()
                .join("assets")
                .join(id)
                .join(ASSETS)
                .join("link.bin");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            assert!(matches!(
                bundles.read_chunk(id, BundleFile::Asset("link.bin"), 0, 6),
                Err(WidgetsError::NotFound)
            ));
        }
        // An unreferenced bundle survives its grace period, then goes.
        assert_eq!(
            bundles.collect(&HashSet::new(), Duration::from_secs(600)),
            0
        );
        assert_eq!(
            bundles.collect(&HashSet::from([id.clone()]), Duration::ZERO),
            0
        );
        assert_eq!(bundles.collect(&HashSet::new(), Duration::ZERO), 1);
    }
}
