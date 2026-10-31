use super::*;

#[derive(Deserialize, Serialize)]
pub(crate) struct ApiRequest {
    pub(crate) schema_version: u32,
    pub(crate) op: String,
    #[serde(default)]
    pub(crate) deployment: Option<String>,
    #[serde(default)]
    pub(crate) role: Option<String>,
    #[serde(default)]
    pub(crate) graph: Option<Deployment>,
    #[serde(default)]
    pub(crate) revision: Option<String>,
    #[serde(default)]
    pub(crate) service_id: Option<String>,
    #[serde(default)]
    pub(crate) cursor: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) log: Option<LogEventInput>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LogEventInput {
    pub(crate) source: String,
    pub(crate) level: String,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) event_code: Option<String>,
}
#[derive(Deserialize, Serialize)]
pub(crate) struct ApiResponse {
    pub(crate) schema_version: u32,
    pub(super) ok: bool,
    pub(super) data: Value,
    pub(super) error: Option<String>,
}
