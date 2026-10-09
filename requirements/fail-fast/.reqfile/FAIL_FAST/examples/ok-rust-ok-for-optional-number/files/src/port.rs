/// The port in `text`, or `None` when it is not a number: callers ask again.
pub fn port(text: &str) -> Option<u16> {
    text.trim().parse::<u16>().ok()
}
