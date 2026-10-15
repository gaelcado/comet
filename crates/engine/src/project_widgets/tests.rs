use std::sync::atomic::Ordering;

use base64::engine::general_purpose::STANDARD as BASE64;
use zeron_proto::{VISUAL_MAX_ASSET_BYTES, VISUAL_MAX_ASSETS};

use super::*;

const SCRIPT: &str = "return ui.text 'v1'";

struct AnySpace;

impl WidgetSpaces for AnySpace {
    fn owned(&self, _: &str) -> bool {
        true
    }
    fn deleted(&self, _: &str) -> bool {
        false
    }
}

fn content(script: &str, assets: &[(&str, &[u8])]) -> ProjectWidgetContentInput {
    ProjectWidgetContentInput {
        script: script.into(),
        assets: assets
            .iter()
            .map(|(name, bytes)| ((*name).to_owned(), BASE64.encode(bytes)))
            .collect(),
        keep_assets_from_bundle: None,
    }
}

fn update(widget: &ProjectWidget, content: ProjectWidgetContentInput) -> UpdateProjectWidgetParams {
    UpdateProjectWidgetParams {
        widget_id: widget.id.clone(),
        expected_revision: widget.revision,
        operation_id: crate::new_id(),
        title: None,
        refresh_recipe: ProjectWidgetRecipePatch::Unchanged,
        content: Some(content),
        refresh_id: None,
        target_device_id: None,
    }
}

fn bundle_dirs(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root.join(PROJECT_WIDGETS_DIR).join("assets"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn script_of(widgets: &ProjectWidgets, widget: &ProjectWidget) -> Vec<u8> {
    let chunk = widgets
        .read_asset_chunk(&ReadProjectWidgetAssetChunkParams {
            widget_id: widget.id.clone(),
            content_revision: widget.content_revision,
            asset_name: PROJECT_WIDGET_SCRIPT_ASSET.into(),
            offset: 0,
            target_device_id: None,
        })
        .unwrap();
    assert!(chunk.done);
    BASE64.decode(chunk.data).unwrap()
}

/// Every failure between copy, publication and commit leaves the widget on
/// its previous bundle, and no bundle half-visible.
#[test]
fn failed_content_writes_keep_the_previous_bundle_and_pointer() {
    let root = tempfile::tempdir().unwrap();
    let widgets = ProjectWidgets::open(root.path(), "device-a", Arc::new(AnySpace));
    let created = widgets
        .create(&CreateProjectWidgetParams {
            space_id: "space".into(),
            creation_key: "kpis".into(),
            operation_id: "op-create".into(),
            title: "KPIs".into(),
            content: content(SCRIPT, &[("data.json", b"[1]")]),
            refresh_recipe: None,
            target_device_id: None,
        })
        .unwrap()
        .widget;
    let before = bundle_dirs(root.path());
    assert_eq!(before, [created.content.bundle_id.as_str()]);

    let missing_asset =
        "return ui.scene { height = 200, draw = function(s) s:model { asset = 'prism.glb' } end }";
    let oversize = vec![0u8; VISUAL_MAX_ASSET_BYTES + 1];
    let too_many: Vec<(String, Vec<u8>)> = (0..=VISUAL_MAX_ASSETS)
        .map(|i| (format!("a{i}.bin"), vec![1]))
        .collect();
    let too_many: Vec<(&str, &[u8])> = too_many
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    for (case, bad) in [
        ("script naming a missing asset", content(missing_asset, &[])),
        (
            "traversal name",
            content(SCRIPT, &[("../escape.bin", b"x")]),
        ),
        ("oversize asset", content(SCRIPT, &[("big.bin", &oversize)])),
        ("too many assets", content(SCRIPT, &too_many)),
        ("blank script", content("  ", &[])),
    ] {
        let err = widgets.update(&update(&created, bad)).unwrap_err();
        assert!(
            matches!(err, WidgetsError::InvalidVisual(_)),
            "{case}: {err}"
        );
    }
    // Keeping assets from a bundle that is not the current one is stale.
    let mut stale = content("return ui.text 'v2'", &[]);
    stale.keep_assets_from_bundle = Some("0".repeat(32));
    assert!(matches!(
        widgets.update(&update(&created, stale)),
        Err(WidgetsError::Conflict)
    ));

    // Published, then the transaction fails: the new bundle never becomes
    // the widget's and does not linger.
    widgets.inner.fail_next_commit.store(true, Ordering::SeqCst);
    let failed = widgets
        .update(&update(&created, content("return ui.text 'v2'", &[])))
        .unwrap_err();
    assert!(matches!(failed, WidgetsError::Write(_)), "{failed}");
    assert_eq!(widgets.get(&created.id).unwrap(), created);
    assert_eq!(
        bundle_dirs(root.path()),
        before,
        "no staging or orphan left"
    );
    assert_eq!(script_of(&widgets, &created), SCRIPT.as_bytes());

    // A real update keeps unchanged assets from the exact current bundle
    // and the widget's seed; the old bundle becomes collectable.
    let mut next = content("return ui.text 'v2'", &[]);
    next.keep_assets_from_bundle = Some(created.content.bundle_id.clone());
    let updated = widgets.update(&update(&created, next)).unwrap().widget;
    assert_eq!(
        (updated.revision, updated.content_revision),
        (created.revision + 1, created.content_revision + 1)
    );
    assert_eq!(updated.content.assets, created.content.assets);
    assert_eq!(updated.content.render_seed, created.content.render_seed);
    assert!(matches!(
        widgets.read_asset_chunk(&ReadProjectWidgetAssetChunkParams {
            widget_id: created.id.clone(),
            content_revision: created.content_revision,
            asset_name: PROJECT_WIDGET_SCRIPT_ASSET.into(),
            offset: 0,
            target_device_id: None,
        }),
        Err(WidgetsError::Conflict)
    ));
    assert_eq!(bundle_dirs(root.path()).len(), 2, "within its grace period");
    assert_eq!(widgets.collect_garbage_older_than(Duration::ZERO), 1);
    assert_eq!(
        bundle_dirs(root.path()),
        [updated.content.bundle_id.as_str()]
    );
    assert_eq!(script_of(&widgets, &updated), b"return ui.text 'v2'");
}

/// A newer build's store is served read-only rather than rewritten.
#[test]
fn a_store_from_a_newer_build_is_never_written() {
    let root = tempfile::tempdir().unwrap();
    {
        let conn = Connection::open(root.path().join(PROJECT_WIDGETS_DB)).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
    }
    let widgets = ProjectWidgets::open(root.path(), "device-a", Arc::new(AnySpace));
    let err = widgets
        .create(&CreateProjectWidgetParams {
            space_id: "space".into(),
            creation_key: "k".into(),
            operation_id: "op".into(),
            title: "t".into(),
            content: content(SCRIPT, &[]),
            refresh_recipe: None,
            target_device_id: None,
        })
        .unwrap_err();
    assert!(matches!(err, WidgetsError::StoreUnavailable), "{err}");
    let version: i64 = Connection::open(root.path().join(PROJECT_WIDGETS_DB))
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 99);
}

/// Saving copies exactly what the chat's visual stored, and refuses a visual
/// whose manifest names a file that is gone or escapes the uploads jail.
#[test]
fn a_saved_visual_is_read_whole_from_the_uploads_jail() {
    let root = tempfile::tempdir().unwrap();
    let uploads = crate::Uploads::from_root(&root.path().join("uploads"));
    let assets = vec![("data.json".to_owned(), b"[1,2]".to_vec())];
    uploads.store_visual_assets("v:one", &assets).unwrap();
    let path = uploads.store_visual("v:one", SCRIPT).unwrap();
    let stored = uploads.read_visual(&path).unwrap();
    assert_eq!(stored.script, SCRIPT);
    assert_eq!(stored.assets["data.json"], b"[1,2]");

    let asset = path.replace("-visual.luau", "-asset-data.json");
    std::fs::remove_file(&asset).unwrap();
    assert!(
        uploads.read_visual(&path).is_err(),
        "a missing asset fails the copy"
    );
    #[cfg(unix)]
    {
        let outside = root.path().join("outside.json");
        std::fs::write(&outside, b"secret").unwrap();
        std::os::unix::fs::symlink(&outside, &asset).unwrap();
        assert!(
            uploads.read_visual(&path).is_err(),
            "a symlink out of the jail"
        );
    }
    let elsewhere = root.path().join("elsewhere-visual.luau");
    std::fs::write(&elsewhere, SCRIPT).unwrap();
    assert!(uploads.read_visual(&elsewhere.to_string_lossy()).is_err());
}
