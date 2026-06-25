fn main() {
    if let Err(err) = greentic_desktop_cli::run_gtc_cli(std::env::args().skip(1)) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
