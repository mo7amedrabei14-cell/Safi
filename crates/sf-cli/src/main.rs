//! SafiData CLI Binary entry point (`safi`).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match sf_cli::run_cli_action(&args) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("خطأ: {err}");
            std::process::exit(1);
        }
    }
}
