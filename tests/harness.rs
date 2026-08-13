//! Gateway harness — spins up the kdrive Go gateway via docker-compose
//! for end-to-end tests that require a real HTTP backend.

use std::process::Command;
use std::time::{Duration, Instant};

const COMPOSE_FILE: &str = "../kdrive/deploy/dev/docker-compose.yml";
const GATEWAY_URL: &str = "http://localhost:8080";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Manages the lifecycle of the kdrive Go gateway for E2E tests.
pub struct GatewayHarness {
    pub base_url: String,
}

impl GatewayHarness {
    /// Start the gateway via docker-compose and wait for readiness.
    /// Returns the base URL if successful.
    pub fn start() -> Result<Self, String> {
        // Start docker compose
        let up_result = Command::new("docker")
            .args(["compose", "-f", COMPOSE_FILE, "up", "-d", "--wait"])
            .output()
            .map_err(|e| format!("failed to run docker compose: {e}"))?;

        if !up_result.status.success() {
            let stderr = String::from_utf8_lossy(&up_result.stderr);
            return Err(format!("docker compose up failed: {stderr}"));
        }

        // Wait for gateway readiness
        let start = Instant::now();
        loop {
            if start.elapsed() > HEALTH_TIMEOUT {
                return Err("gateway did not become ready within 60s".to_string());
            }

            if Self::is_healthy() {
                return Ok(Self {
                    base_url: GATEWAY_URL.to_string(),
                });
            }

            std::thread::sleep(HEALTH_POLL_INTERVAL);
        }
    }

    /// Check if the gateway is responding to /readyz.
    fn is_healthy() -> bool {
        Command::new("curl")
            .args(["-sf", &format!("{GATEWAY_URL}/readyz")])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Tear down the gateway.
    pub fn stop(&self) {
        let _ = Command::new("docker")
            .args(["compose", "-f", COMPOSE_FILE, "down", "-v"])
            .output();
    }
}

impl Drop for GatewayHarness {
    fn drop(&mut self) {
        self.stop();
    }
}
