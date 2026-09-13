/// The config
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct Config {
    pub port: u16,
}
