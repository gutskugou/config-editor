use config_editor::cli;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match cli::run(&args) {
        Ok(Some(())) => {}
        Ok(None) => {
            println!("TUI coming in Task 10");
        }
        Err(e) => {
            eprintln!("config-editor: {e}");
            std::process::exit(1);
        }
    }
}
