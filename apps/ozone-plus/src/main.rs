fn main() {
    if let Err(error) = ozone_plus::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}