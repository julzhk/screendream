fn main() {
    if let Err(error) = screendream::app::run() {
        eprintln!("Screendream could not start: {error}");
    }
}
