//! Isolated native sidebar review fixture. ZERON_SIDEBAR_COMPACT / ZERON_SIDEBAR_HIDE_LABEL select layout.
use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use zeron_ui::*;

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let temp = tempfile::tempdir()?;
    let output = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let data = temp.path().to_path_buf();
    eprintln!("PID={} executable={:?} data={:?}",std::process::id(),std::env::current_exe(),data);
    gpui_platform::application().with_assets(icons::Assets).run(move |cx| {
        gpui_tokio::init(cx); gpui_base::init(cx);
        let mut settings = settings::UiSettings::default();
        settings.sidebar_show_branch = true;
        settings.sidebar_compact = std::env::var_os("ZERON_SIDEBAR_COMPACT").is_some();
        settings.sidebar_show_project_label = std::env::var_os("ZERON_SIDEBAR_HIDE_LABEL").is_none();
        settings.sidebar_organization = settings::SidebarOrganization::InOneList;
        settings.sidebar_width = 310.0;
        settings.sidebar_pins_mut("local".into()).extend(["chat-0".into(), "chat-1".into()]);
        let project_path = data.join("fieldnotes");
        std::fs::create_dir_all(project_path.join("public")).unwrap();
        std::fs::write(project_path.join("public/favicon.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="1" y="1" width="22" height="22" rx="6" fill="#668cf5"/><path d="M7 6h11v3h-8v3h6v3h-6v4H7Z" fill="white"/></svg>"##).unwrap();
        settings.surface = zeron_theme::SurfacePreference::Frosted;
        settings.save(&data).unwrap();
        settings::init(settings.clone(), data.clone(), cx);
        let fonts = typography::register_fonts(cx);
        typography::init(settings.ui_font_family.clone(), settings.ui_font_size, settings.terminal_font_family.clone(), settings.terminal_font_size, settings.code_font_family.clone(), settings.code_font_size, fonts, cx);
        theme_library::init(data.clone(), cx);
        appearance::init(if std::env::var_os("ZERON_PALETTE_LIGHT").is_some() { appearance::AppearanceMode::Light } else { appearance::AppearanceMode::Dark }, settings.theme_selection, settings.accent, settings.surface, cx);
        history::init(settings.git_history_columns, settings.git_history_column_widths,
            settings.git_history_column_order, settings.git_history_author_display, cx);
        composer::init(cx, settings.composer_send_behavior); terminal::panel::init(cx); app_menus::init(cx);
        let state = cx.new(|_| {
            let mut s = state::AppState::new();
            s.connection = zeron_proto::view::ConnectionStatus::Ready;
            s.workspace_scope = Some(zeron_proto::WorkspaceScope::Local);
            if std::env::var_os("ZERON_SIDEBAR_ACCOUNT").is_some() {
                s.workspace_scope = Some(zeron_proto::WorkspaceScope::Synced);
                s.auth = Some(zeron_proto::AuthState::SignedIn {
                    user: zeron_proto::UserProfile { id: "fixture-user".into(), email: "alex@example.test".into(), name: Some("Alex".into()) },
                    org_id: Some("fixture-org".into()),
                });
            }
            s.local_device_id = Some("local".into());
            s.devices = vec![serde_json::from_value(serde_json::json!({"id":"fixture","name":"This Mac","platform":std::env::consts::OS,"lastSeenAt":null})).unwrap()];
            s.selected_chat = Some("browser-fixture".into()); s.selected_space = Some("project".into());
            s.auto_selected = true; s.chats_synced = true; s.spaces_synced = true;
            s.spaces = vec![serde_json::from_value(serde_json::json!({"id":"project","deviceId":"local","path":project_path,"createdAt":"2026-09-08T00:00:00Z"})).unwrap()];
            s.chats = vec![serde_json::from_value(serde_json::json!({"id":"browser-fixture","deviceId":"local","spaceId":"project","title":"Build the Fieldnotes workspace","archived":false,"createdAt":"2026-09-08T00:00:00Z","config":{"harness":"claude-code","model":"claude-sonnet-4-6","reasoning":null,"sandbox":"workspace-write"}})).unwrap()];

            s.chats.clear(); s.spaces.clear(); s.selected_chat = None; s.selected_space = None;
            s
        });
        let boot = EngineBootConfig { data_dir: data, ipc_port: 0, edge_url: String::new(), edge_token: None, org_id: None, workos_client_id: None, default_harness: HarnessId::ClaudeCode };
        let window = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(gpui::point(px(12.),px(30.)), size(px(960.),px(720.))))),
            titlebar: Some(gpui::TitlebarOptions { title: None, appears_transparent: true, traffic_light_position: Some(gpui::point(px(14.),px(14.))) }),
            app_owns_titlebar_drag: true,
            ..Default::default()
        }, |_, cx| cx.new(|cx| shell::Shell::new(state.clone(), boot, cx))).unwrap();
        state.update(cx, |_, cx| cx.notify());
        cx.activate(true);
        cx.spawn(async move |cx| {
            for (name, count, expanded, active) in [("desktop-compact",5,false,false),("desktop-expanded",5,true,false),("desktop-scrolled",5,true,true),("desktop-active",1,false,true),("desktop-two-agents",2,true,false)] {
                window.update(cx, |s,_,cx|s.fixture_agent_updates(count,expanded,active,cx)).unwrap();
                cx.background_executor().timer(std::time::Duration::from_millis(1200)).await;
                gpui::AnyWindowHandle::from(window).update(cx, |_,w,cx| {w.draw(cx).clear(); w.render_to_image().unwrap().save(output.join(format!("{name}.png"))).unwrap();}).unwrap();
            }
            cx.update(|cx|cx.quit());
        }).detach();
    });
    Ok(())
}
