//! Calls to Jev through its System One endpoint.

use std::thread;
use std::time::Duration;

use ureq::Agent;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const ATTEMPTS: u32 = 3;
/// Statuses worth retrying: rate limits, overload, and transient gateway or edge failures.
const RETRY_STATUSES: &[u16] = &[429, 502, 503, 504, 520, 522, 524, 529];

pub struct Client {
    agent: Agent,
    url: String,
    api_key: String,
}

impl Client {
    pub fn new(endpoint: &str, api_key: String) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            agent,
            url: format!("{}/systemone", endpoint.trim_end_matches('/')),
            api_key,
        }
    }

    /// Sends one request and returns the response body.
    pub fn ask(&self, request: &serde_json::Value) -> Result<String, String> {
        let mut last_error = String::new();
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                thread::sleep(Duration::from_millis(500 * 2u64.pow(attempt - 1)));
            }
            let response = self
                .agent
                .post(&self.url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .send_json(request);
            let mut response = match response {
                Ok(response) => response,
                Err(e) => {
                    last_error = format!("Jev request failed: {e}");
                    continue;
                }
            };
            let status = response.status().as_u16();
            let body = response
                .body_mut()
                .read_to_string()
                .map_err(|e| format!("cannot read Jev response: {e}"))?;
            match status {
                200 => return Ok(body),
                s if RETRY_STATUSES.contains(&s) => {
                    last_error = format!("Jev returned HTTP {s}: {}", body.trim())
                }
                s => return Err(format!("Jev returned HTTP {s}: {}", body.trim())),
            }
        }
        Err(last_error)
    }
}
