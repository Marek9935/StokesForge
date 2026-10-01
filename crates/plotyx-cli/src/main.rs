//! Binaire headless : voir [`plotyx_cli`].

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    if let Err(message) = plotyx_cli::run(&args, &mut stdout) {
        eprintln!("plotyx-cli: {message}");
        std::process::exit(1);
    }
}
