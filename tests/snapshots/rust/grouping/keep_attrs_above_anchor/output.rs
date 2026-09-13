/// The config
#[serde(rename_all = "camelCase")]
#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
}
