use super::app::{App, Focus, Prompt};
use crate::core;
use crate::domain::{Application, Capability, Format, Setting, Source};
use crate::i18n;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};

// 进程级环境变量 EDITOR/VISUAL 是共享全局；注入编辑器的测试必须串行执行
static EDITOR_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn sample_apps() -> Vec<Application> {
    vec![Application {
        id: "git".into(),
        name: "Git".into(),
        name_zh: "Git".into(),
        description: "d".into(),
        description_zh: "d".into(),
        command: Some("git".into()),
        installed: true,
        capabilities: vec![Capability::Structured],
        sources: vec![Source {
            path: "/home/me/.gitconfig".into(),
            resolved: None,
            exists: true,
            format: Format::Git,
            diagnostic: None,
            settings: vec![Setting {
                key: "user.name".into(),
                value: "Ada".into(),
                line: 1,
                occ: 1,
                editable: true,
                sensitive: false,
            }],
        }],
    }]
}

#[test]
fn j_navigates_apps() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.app_index, 0); // 只有一个应用，不动
    assert_eq!(app.focus, Focus::Apps);
}

#[test]
fn right_traverses_sources_then_settings() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.focus, Focus::Sources);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.focus, Focus::Settings);
    assert_eq!(app.setting_index, 0);
}

#[test]
fn search_filters_apps() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Char('/')));
    assert_eq!(app.prompt, Prompt::Search);
    app.handle_key(key(KeyCode::Char('g')));
    app.handle_key(key(KeyCode::Char('i')));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.filter, "gi");
    assert_eq!(app.prompt, Prompt::None);
    assert_eq!(app.filtered().len(), 1);
}

#[test]
fn esc_from_settings_returns_to_apps() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.focus, Focus::Sources);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.focus, Focus::Apps);
}

#[test]
fn render_smoke_contains_key_labels() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.width = 80;
    app.height = 24;
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(text.contains("Config Editor"));
    assert!(text.contains("user.name"));
    assert!(text.contains("q quit"));
}

#[test]
fn render_shows_sources_section_and_only_selected_source_settings() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.apps[0].sources[0].settings = vec![Setting {
        key: "user.first".into(),
        value: "A".into(),
        line: 1,
        occ: 1,
        editable: true,
        sensitive: false,
    }];
    app.apps[0].sources.push(Source {
        path: "/home/me/.gitconfig.extra".into(),
        resolved: None,
        exists: true,
        format: Format::Git,
        diagnostic: None,
        settings: vec![Setting {
            key: "user.second".into(),
            value: "B".into(),
            line: 1,
            occ: 1,
            editable: true,
            sensitive: false,
        }],
    });
    app.focus = Focus::Sources;
    app.source_index = 1;
    app.width = 80;
    app.height = 24;
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(text.contains(".gitconfig"), "source 路径必须可见:\n{text}");
    assert!(text.contains(".gitconfig.extra"));
    assert!(
        text.contains("user.second"),
        "设置区必须显示选中 source 的设置:\n{text}"
    );
    assert!(
        !text.contains("user.first"),
        "设置区不得混入其他 source 的设置:\n{text}"
    );
    let style = row_starting_with(&buffer, "> ").expect("选中 source 行必须以 '> ' 开头");
    assert_eq!(style.fg, Some(Color::Green), "选中 source 行必须高亮为绿色");
    assert!(
        style.add_modifier.contains(Modifier::BOLD),
        "选中 source 行必须加粗"
    );
}

fn row_starting_with(buffer: &ratatui::buffer::Buffer, prefix: &str) -> Option<Style> {
    let width = buffer.area.width as usize;
    for y in 0..buffer.area.height {
        let row: String = (0..width)
            .map(|x| buffer.cell((x as u16, y)).map(|c| c.symbol()).unwrap_or(""))
            .collect();
        if row.starts_with(prefix) {
            return buffer.cell((0, y)).map(|c| c.style());
        }
    }
    None
}

#[test]
fn q_requests_quit() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.quit);
}

#[test]
fn ctrl_c_in_search_quits() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Char('/')));
    assert_eq!(app.prompt, Prompt::Search);
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.quit);
}

#[test]
fn q_is_text_input_inside_search() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(key(KeyCode::Char('q')));
    assert!(!app.quit);
    assert_eq!(app.input, "q");
}

fn temp_env() -> (tempfile::TempDir, core::Manager, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let cfg = home.join(".gitconfig");
    std::fs::write(&cfg, b"[user]\nname = Ada\n").unwrap();
    let manager = core::Manager {
        home: home.clone(),
        config_root: dir.path().join("config"),
        state_root: dir.path().join("state"),
    };
    (dir, manager, cfg)
}

fn app_with_source(manager: core::Manager, cfg: &std::path::Path) -> App {
    let mut app = App::new(sample_apps(), manager, i18n::Catalog { chinese: false });
    app.apps[0].sources[0].path = cfg.to_str().unwrap().into();
    app.apps[0].sources[0].resolved = Some(cfg.to_str().unwrap().into());
    app.apps[0].sources[0].settings[0].line = 2;
    app
}

#[test]
fn structured_edit_stages_replacement_and_builds_diff() {
    let (_dir, manager, cfg) = temp_env();
    let mut app = app_with_source(manager, &cfg);
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('s')));
    assert_eq!(app.prompt, Prompt::Value);
    assert_eq!(app.input, "Ada");
    for _ in 0..3 {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "Grace".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.prompt, Prompt::Confirm);
    let change = app.pending.as_ref().expect("pending change");
    let stage = change.stage.clone();
    assert_eq!(std::fs::read(&stage).unwrap(), b"[user]\nname = Grace\n");
    assert!(app.diff.contains("name = Ada"));
    assert!(app.diff.contains("name = Grace"));
    let _ = app.manager.discard(&app.pending.take().unwrap());
    assert!(!stage.exists());
}

#[test]
fn diff_scroll_clamps_at_bottom() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    let mut diff = String::new();
    for i in 0..30 {
        diff.push_str(&format!("+line {i}\n"));
    }
    app.prompt = Prompt::Confirm;
    app.diff = diff;
    app.height = 12;
    let max = app.max_diff_offset();
    assert!(max > 0);
    // 远超底部的下滚：offset 必须被钳制到上限
    for _ in 0..100 {
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    }
    assert_eq!(app.diff_offset, max, "offset must clamp at max");
    // 钳制后按一次 k 立即上移
    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.diff_offset, max - 1);
}

#[test]
fn structured_edit_relocates_stale_line_number() {
    let (dir, manager, cfg) = temp_env();
    // 扫描之后、编辑之前，外部工具在 name 前插入了一行 email
    std::fs::write(&cfg, b"[user]\nemail = ada@example.test\nname = Ada\n").unwrap();
    let mut app = app_with_source(manager, &cfg);
    // app 中保存的仍是扫描时的旧行号：user.name 在第 2 行
    assert_eq!(app.apps[0].sources[0].settings[0].line, 2);
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('s')));
    for _ in 0..3 {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "Grace".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        app.prompt,
        Prompt::Confirm,
        "stale line must not abort edit"
    );
    let change = app.pending.as_ref().expect("pending change");
    let text = String::from_utf8(std::fs::read(&change.stage).unwrap()).unwrap();
    assert!(
        text.contains("name = Grace"),
        "user.name 行必须被修改（当前第 3 行）:\n{text}"
    );
    assert!(
        text.contains("email = ada@example.test"),
        "email 行（旧行号 2 指向它）不得被修改:\n{text}"
    );
    assert!(!text.contains("email = Grace"), "email 被错误修改:\n{text}");
    let _ = app.manager.discard(&app.pending.take().unwrap());
    let _ = dir;
}

#[test]
fn structured_edit_targets_selected_duplicate_occurrence() {
    let (dir, manager, cfg) = temp_env();
    // 同名键两条：选中第二条（user.name, occ=2）并改值，第一条必须原样保留
    std::fs::write(&cfg, b"[user]\nname = Ada\nname = Grace\n").unwrap();
    let mut app = app_with_source(manager, &cfg);
    app.apps[0].sources[0].settings = vec![
        Setting {
            key: "user.name".into(),
            value: "Ada".into(),
            line: 2,
            occ: 1,
            editable: true,
            sensitive: false,
        },
        Setting {
            key: "user.name".into(),
            value: "Grace".into(),
            line: 3,
            occ: 2,
            editable: true,
            sensitive: false,
        },
    ];
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('j'))); // 移到第二条
    app.handle_key(key(KeyCode::Char('s')));
    assert_eq!(app.input, "Grace");
    for _ in 0..5 {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "Rust".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.prompt, Prompt::Confirm, "duplicate must not abort edit");
    let change = app.pending.as_ref().expect("pending change");
    let text = String::from_utf8(std::fs::read(&change.stage).unwrap()).unwrap();
    assert!(
        text.contains("name = Ada"),
        "第一条同名键不得被修改:\n{text}"
    );
    assert!(
        text.contains("name = Rust"),
        "选中的第二条必须被修改:\n{text}"
    );
    assert!(!text.contains("name = Grace"), "第二条已改为 Rust:\n{text}");
    let _ = app.manager.discard(&app.pending.take().unwrap());
    let _ = dir;
}

#[test]
fn structured_edit_rejects_ambiguous_duplicates() {
    let (dir, manager, cfg) = temp_env();
    // 扫描时第 3 条 user.name（值 Grace）被选中；外部改动后同名键仍有多条、
    // 值 Grace 出现两次、occ 无法命中选中条 → 拒绝并要求重新扫描
    std::fs::write(&cfg, b"[user]\nname = Grace\nname = Grace\nname = Ada\n").unwrap();
    let mut app = app_with_source(manager, &cfg);
    app.apps[0].sources[0].settings[0].occ = 3;
    app.apps[0].sources[0].settings[0].line = 4;
    app.apps[0].sources[0].settings[0].value = "Grace".into();
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('s')));
    assert_eq!(app.prompt, Prompt::Value, "编辑前不应被拒");
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.prompt, Prompt::None, "歧义必须中止编辑");
    assert!(
        app.status.contains("re-scan"),
        "必须提示重新扫描: {}",
        app.status
    );
    assert!(app.pending.is_none(), "歧义时不得留下 pending 暂存");
    let edit_dir = dir.path().join("state/config-editor/edit");
    let leftovers: Vec<_> = std::fs::read_dir(&edit_dir)
        .map(|rd| rd.filter_map(Result::ok).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "暂存必须被丢弃");
    let _ = dir;
}

#[test]
fn ctrl_c_in_confirm_quits_and_discards_stage() {
    let (dir, manager, cfg) = temp_env();
    let mut app = app_with_source(manager, &cfg);
    let change = app.manager.prepare(&cfg, Format::Git).unwrap();
    let stage = change.stage.clone();
    app.pending = Some(change);
    app.prompt = Prompt::Confirm;
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.quit);
    assert!(app.pending.is_none());
    assert!(
        !stage.exists(),
        "stage must be discarded when quitting from confirm"
    );
    let edit_dir = dir.path().join("state/config-editor/edit");
    let leftovers: Vec<_> = std::fs::read_dir(&edit_dir)
        .map(|rd| rd.filter_map(Result::ok).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "edit directory must be empty");
}

#[test]
fn editor_parse_error_discards_stage_and_reports_status() {
    // 与 j_moves 测试串行化：两个测试共享进程环境变量 EDITOR/VISUAL
    let _env_guard = EDITOR_ENV_LOCK.lock().unwrap();
    let (dir, manager, cfg) = temp_env();
    let mut app = app_with_source(manager, &cfg);
    let saved_visual = std::env::var_os("VISUAL");
    let saved_editor = std::env::var_os("EDITOR");
    std::env::remove_var("VISUAL");
    std::env::set_var("EDITOR", "'");
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('e')));
    match saved_visual {
        Some(v) => std::env::set_var("VISUAL", v),
        None => std::env::remove_var("VISUAL"),
    }
    match saved_editor {
        Some(v) => std::env::set_var("EDITOR", v),
        None => std::env::remove_var("EDITOR"),
    }
    assert!(app.status.starts_with('!'), "status must report the error");
    let edit_dir = dir.path().join("state/config-editor/edit");
    let leftovers: Vec<_> = std::fs::read_dir(&edit_dir)
        .map(|rd| rd.filter_map(Result::ok).collect())
        .unwrap_or_default();
    assert!(
        leftovers.is_empty(),
        "staged file must be discarded on editor error"
    );
}

#[test]
fn selected_detail_row_counts_source_headers_and_diagnostics() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.apps[0].sources[0].settings = vec![
        Setting {
            key: "k1".into(),
            ..Default::default()
        },
        Setting {
            key: "k2".into(),
            ..Default::default()
        },
        Setting {
            key: "k3".into(),
            ..Default::default()
        },
    ];
    app.setting_index = 0;
    assert_eq!(
        app.selected_detail_row(&app.apps[0].sources[0]),
        1,
        "[file] 占 1 行"
    );
    app.setting_index = 2;
    assert_eq!(app.selected_detail_row(&app.apps[0].sources[0]), 3);
    // 诊断行让设置行号 +1
    app.apps[0].sources[0].diagnostic = Some("boom".into());
    app.setting_index = 0;
    assert_eq!(app.selected_detail_row(&app.apps[0].sources[0]), 2);
    app.setting_index = 1;
    assert_eq!(app.selected_detail_row(&app.apps[0].sources[0]), 3);
    // 第二个 source 独立计算行号：1([file]) + 1(诊断) = 2 行前缀
    let second = Source {
        path: "/home/me/.gitconfig.extra".into(),
        resolved: None,
        exists: true,
        format: Format::Git,
        diagnostic: Some("x".into()),
        settings: vec![Setting {
            key: "k4".into(),
            ..Default::default()
        }],
    };
    app.apps[0].sources.push(second);
    app.setting_index = 0;
    assert_eq!(
        app.selected_detail_row(&app.apps[0].sources[1]),
        2,
        "1([file]) + 1(诊断) = 2 行前缀"
    );
}

#[test]
fn detail_viewport_keeps_bottom_setting_visible_on_small_terminals() {
    // 30 个设置 + 头 7 行（标题/应用/描述/Sources 分区，公式 1+apps+2+2+visible_sources）：
    // 80x16 终端中部 10 行中 detail 区仅 3 行时，
    // 滚动视口必须包含选中的最后一个设置（当前实现按整块高度计算，底部被裁剪）
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.apps[0].sources[0].settings = (0..30)
        .map(|i| Setting {
            key: format!("user.k{i}"),
            value: format!("v{i}"),
            line: 1,
            occ: 1,
            editable: true,
            sensitive: false,
        })
        .collect();
    app.focus = Focus::Settings;
    app.setting_index = 29;
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(
        text.contains("user.k29"),
        "选中的最后一个设置必须在视口内可见:\n{text}"
    );
    assert!(
        text.contains("> user.k29"),
        "选中标记必须与最后一个设置同行:\n{text}"
    );
}

#[test]
fn arrow_keys_traverse_three_levels_of_focus() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.focus, Focus::Sources);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.focus, Focus::Settings);
    assert_eq!(app.setting_index, 0);
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.focus, Focus::Sources);
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.focus, Focus::Apps);
    // 最顶层 Left/Esc 不动作
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.focus, Focus::Apps);
}

#[test]
fn entering_sources_resets_source_index() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    // 两个 source，先选中第二个再退出重进
    app.apps[0].sources.push(Source {
        path: "/home/me/.gitconfig.extra".into(),
        resolved: None,
        exists: true,
        format: Format::Git,
        diagnostic: None,
        settings: vec![],
    });
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.source_index, 1);
    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.source_index, 0, "重进 Sources 必须重置索引");
}

#[test]
fn j_moves_between_sources_and_edits_target_the_selected_one() {
    // 与 editor_parse_error 测试串行化：两个测试共享进程环境变量 EDITOR/VISUAL
    let _env_guard = EDITOR_ENV_LOCK.lock().unwrap();
    let (dir, manager, cfg) = temp_env();
    let extra = dir.path().join("home/.gitconfig.extra");
    std::fs::write(&extra, b"[user]\nname = Grace\n").unwrap();
    let saved_visual = std::env::var_os("VISUAL");
    let saved_editor = std::env::var_os("EDITOR");
    std::env::remove_var("VISUAL");
    std::env::set_var("EDITOR", "sed -i s/Grace/GraceX/");
    let mut app = app_with_source(manager, &cfg);
    app.apps[0].sources.push(Source {
        path: extra.to_str().unwrap().into(),
        resolved: Some(extra.to_str().unwrap().into()),
        exists: true,
        format: Format::Git,
        diagnostic: None,
        settings: vec![],
    });
    app.handle_key(key(KeyCode::Right)); // Sources
    app.handle_key(key(KeyCode::Char('j'))); // 第二个 source
    assert_eq!(app.source_index, 1);
    app.handle_key(key(KeyCode::Right)); // Settings（第二个 source 无设置 → 提示）
    assert_eq!(app.focus, Focus::Sources, "无设置的 source 不进入 Settings");
    assert!(app.status.contains("No structured settings") || app.status.contains("没有结构化设置"));
    // 用 e 编辑第二个 source（focus 停在 Sources，选中第二个）
    app.handle_key(key(KeyCode::Char('e')));
    let change = app.pending.as_ref().expect("pending change");
    let stage = change.stage.clone();
    let text = String::from_utf8(std::fs::read(&stage).unwrap()).unwrap();
    assert!(
        text.contains("name = Grace"),
        "必须编辑选中的第二个 source:\n{text}"
    );
    let _ = app.manager.discard(&app.pending.take().unwrap());
    match saved_visual {
        Some(v) => std::env::set_var("VISUAL", v),
        None => std::env::remove_var("VISUAL"),
    }
    match saved_editor {
        Some(v) => std::env::set_var("EDITOR", v),
        None => std::env::remove_var("EDITOR"),
    }
    let _ = dir;
}

#[test]
fn s_in_sources_layer_prompts_to_enter_settings() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('s')));
    assert_eq!(app.prompt, Prompt::None);
    assert!(!app.status.is_empty(), "必须给出提示");
}

#[test]
fn s_e_r_in_apps_layer_prompts_to_enter_sources() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    for code in [KeyCode::Char('s'), KeyCode::Char('e'), KeyCode::Char('r')] {
        app.handle_key(key(code));
        assert_eq!(
            app.prompt,
            Prompt::None,
            "Apps 层按 {code:?} 不得进入编辑流程"
        );
        assert!(!app.status.is_empty(), "Apps 层按 {code:?} 必须给出提示");
        assert!(app.pending.is_none(), "Apps 层按 {code:?} 不得留下暂存");
    }
}

fn many_apps(count: usize) -> Vec<Application> {
    (0..count)
        .map(|i| Application {
            id: format!("app-{i}"),
            name: format!("App {i}"),
            ..Default::default()
        })
        .collect()
}

#[test]
fn long_app_list_scrolls_selected_into_view() {
    let mut app = App::new(
        many_apps(12),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.app_index = 11;
    app.focus = Focus::Apps;
    let backend = ratatui::backend::TestBackend::new(80, 14);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(
        text.contains("App 11"),
        "选中的最后一个应用必须滚动进视口:\n{text}"
    );
    assert!(
        !text.contains("App 0"),
        "视口外的应用不得渲染（独立滚动状态）:\n{text}"
    );
}

#[test]
fn diff_renders_with_per_line_styles() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.prompt = Prompt::Confirm;
    app.diff = "--- current\n+++ proposed\n@@ -1 +1 @@\n-old\n+new\n".to_string();
    app.height = 24;
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    assert_eq!(
        row_starting_with(&buffer, "-old").and_then(|s| s.fg),
        Some(Color::Red),
        "删除行必须为红色"
    );
    assert_eq!(
        row_starting_with(&buffer, "+new").and_then(|s| s.fg),
        Some(Color::Green),
        "新增行必须为绿色"
    );
    assert_eq!(
        row_starting_with(&buffer, "@@").and_then(|s| s.fg),
        Some(Color::Cyan),
        "hunk 头必须为青色"
    );
    assert_eq!(
        row_starting_with(&buffer, "+++ proposed").and_then(|s| s.fg),
        Some(Color::Magenta),
        "文件头必须为洋红"
    );
}

#[test]
fn error_detail_view_shows_full_error_and_closes() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.status = "! first line\nsecond line of error".into();
    app.handle_key(key(KeyCode::Char('d')));
    assert!(app.error_view, "d 必须打开错误详情视图");
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(
        text.contains("second line of error"),
        "完整错误详情必须可见:\n{text}"
    );
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.error_view);
    assert!(app.status.is_empty(), "关闭错误视图应清空状态");
}

#[test]
fn error_view_scrolls_with_j_and_k() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.status = "! line1\nline2\nline3\nline4\nline5\nline6".into();
    app.handle_key(key(KeyCode::Char('d')));
    assert!(app.error_view);
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.error_offset, 1);
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.error_offset, 2);
    app.handle_key(key(KeyCode::Char('k')));
    assert_eq!(app.error_offset, 1);
    // 顶部钳制
    for _ in 0..10 {
        app.handle_key(key(KeyCode::Char('k')));
    }
    assert_eq!(app.error_offset, 0);
    // Enter 也可关闭
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.error_view);
}

#[test]
fn small_terminal_blocks_destructive_confirm() {
    // 小终端下 diff/预览不可见：y 不得应用或恢复
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.terminal_small = true;
    app.prompt = Prompt::Confirm;
    app.diff = "-a\n+b\n".into();
    app.pending = Some(core::Change {
        target: "/tmp/x".into(),
        stage: "/tmp/x.stage".into(),
        base_hash: "h".into(),
        identity: core::secure::Identity { dev: 0, ino: 0 },
        mode: 0o600,
        format: Format::Git,
    });
    app.handle_key(key(KeyCode::Char('y')));
    assert_eq!(
        app.prompt,
        Prompt::Confirm,
        "小终端不得应用变更（diff 不可见）"
    );
    assert!(app.pending.is_some(), "pending 不得被消费");
    assert!(!app.status.is_empty(), "必须给出提示");
    // n 取消仍然允许
    app.handle_key(key(KeyCode::Char('n')));
    assert_eq!(app.prompt, Prompt::None);
    assert!(app.pending.is_none());
    // Restore 预览同理
    let mut app2 = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app2.terminal_small = true;
    app2.prompt = Prompt::Restore;
    app2.handle_key(key(KeyCode::Char('y')));
    assert_eq!(app2.prompt, Prompt::Restore, "小终端不得执行恢复");
}

#[test]
fn restore_cancel_variants_leave_no_dangling_state() {
    let (dir, manager, cfg) = temp_env();
    let change = manager.prepare(&cfg, Format::Git).unwrap();
    manager.apply(&change).unwrap();
    for cancel in [KeyCode::Esc, KeyCode::Char('q'), KeyCode::Char('N')] {
        let mut app = app_with_source(manager.clone(), &cfg);
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.prompt, Prompt::Restore);
        app.handle_key(key(cancel));
        assert_eq!(app.prompt, Prompt::None, "{cancel:?} 必须取消恢复");
        assert!(app.restore_snapshot.is_none());
        assert!(app.pending.is_none());
    }
    let _ = dir;
}

#[test]
fn restore_without_snapshot_reports_error() {
    let (dir, manager, cfg) = temp_env();
    // 从未应用过：无快照
    let mut app = app_with_source(manager, &cfg);
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.prompt, Prompt::None, "无快照不得进入预览");
    assert!(!app.status.is_empty(), "必须报告无快照");
    let _ = dir;
}

#[test]
fn restore_preview_uses_canonical_path_like_confirm() {
    // symlink 且扫描时 resolved 缺失：预览与确认必须都命中同一快照键
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let real = home.join(".gitconfig.real");
    std::fs::write(&real, b"[user]\nname = Ada\n").unwrap();
    let link = home.join(".gitconfig");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let manager = core::Manager {
        home: home.clone(),
        config_root: dir.path().join("config"),
        state_root: dir.path().join("state"),
    };
    let change = manager.prepare(&link, Format::Git).unwrap();
    manager.apply(&change).unwrap();
    let mut app = App::new(sample_apps(), manager, i18n::Catalog { chinese: false });
    app.apps[0].sources[0].path = link.to_str().unwrap().into();
    app.apps[0].sources[0].resolved = None; // 模拟扫描时 canonicalize 失败
    app.apps[0].sources[0].exists = true;
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(
        app.prompt,
        Prompt::Restore,
        "symlink 且 resolved 缺失时必须能预览快照"
    );
    app.handle_key(key(KeyCode::Char('y')));
    assert_eq!(app.prompt, Prompt::Confirm);
    assert!(app.pending.is_some());
    let _ = app.manager.discard(&app.pending.take().unwrap());
    let _ = dir;
}

#[test]
fn error_detail_view_requires_error_status() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    app.handle_key(key(KeyCode::Char('d')));
    assert!(!app.error_view, "无错误状态时 d 不得打开详情视图");
    assert!(!app.status.is_empty(), "必须给出提示");
}

#[test]
fn restore_shows_preview_then_confirm_diff() {
    let (dir, manager, cfg) = temp_env();
    // 先应用一次，创建快照
    let change = manager.prepare(&cfg, Format::Git).unwrap();
    manager.apply(&change).unwrap();
    let mut app = app_with_source(manager, &cfg);
    app.handle_key(key(KeyCode::Right)); // Sources
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(
        app.prompt,
        Prompt::Restore,
        "r 必须先展示快照预览而非直接进 diff"
    );
    assert!(app.restore_snapshot.is_some());
    assert!(app.pending.is_none(), "预览阶段不得创建暂存");
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(
        text.contains("Restore snapshot"),
        "必须显示快照预览:\n{text}"
    );
    assert!(text.contains("SHA-256"), "预览必须显示摘要:\n{text}");
    assert!(text.contains("Source"), "预览必须显示来源:\n{text}");
    // y 进入统一 diff 确认
    app.handle_key(key(KeyCode::Char('y')));
    assert_eq!(app.prompt, Prompt::Confirm);
    assert!(app.pending.is_some());
    assert!(app.restore_snapshot.is_none());
    let _ = app.manager.discard(&app.pending.take().unwrap());
    let _ = dir;
}

#[test]
fn restore_preview_can_be_cancelled() {
    let (dir, manager, cfg) = temp_env();
    let change = manager.prepare(&cfg, Format::Git).unwrap();
    manager.apply(&change).unwrap();
    let mut app = app_with_source(manager, &cfg);
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.prompt, Prompt::Restore);
    app.handle_key(key(KeyCode::Char('n')));
    assert_eq!(app.prompt, Prompt::None);
    assert!(app.restore_snapshot.is_none());
    assert!(app.pending.is_none());
    assert!(!app.status.is_empty(), "取消必须给出提示");
    let _ = dir;
}

#[test]
fn tiny_terminal_shows_resize_hint() {
    let mut app = App::new(
        sample_apps(),
        core::Manager::default(),
        i18n::Catalog { chinese: false },
    );
    let backend = ratatui::backend::TestBackend::new(30, 10);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(
        text.contains("Terminal too small") || text.contains("终端窗口过小"),
        "小终端必须显示明确提示:\n{text}"
    );
    assert!(
        text.contains("40x12") || text.contains("40×12"),
        "最小尺寸信息不得被截断:\n{text}"
    );
}
