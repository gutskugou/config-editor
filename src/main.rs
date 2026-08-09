use config_editor::{cli, core, discovery, i18n, paths, tui};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match cli::run(&args) {
        Ok(Some(())) => {}
        Ok(None) => {
            let p = match paths::resolve() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("config-editor: {e}");
                    std::process::exit(1);
                }
            };
            let apps = match discovery::scan(&p) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("config-editor: {e}");
                    std::process::exit(1);
                }
            };
            let manager = core::Manager {
                home: p.home.clone(),
                config_root: p.config.clone(),
                state_root: p.state.clone(),
            };
            let lang = i18n::detect();
            if let Err(e) = tui::run_tui(apps, manager, lang) {
                eprintln!("config-editor: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("config-editor: {e}");
            std::process::exit(1);
        }
    }
}
