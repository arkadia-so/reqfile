/// The settings in `path`: reading and parsing are one job, loading.
pub fn load(path: &Path) -> Result<Settings, Error> {
    let text = fs::read_to_string(path)?;
    let settings = toml::from_str(&text)?;
    Ok(settings)
}
