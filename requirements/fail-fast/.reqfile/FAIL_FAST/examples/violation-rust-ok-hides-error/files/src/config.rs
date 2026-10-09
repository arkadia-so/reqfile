pub fn load(path: &str) -> Config {
    let text = std::fs::read_to_string(path).ok();
    parse(&text.unwrap_or_default())
}
