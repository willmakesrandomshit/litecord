use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    RateLimited,
    Error(String),
}

impl ConnectionState {
    pub fn label(&self) -> String {
        match self {
            Self::Disconnected => "Offline / Local Demo".into(),
            Self::Connecting => "Connecting...".into(),
            Self::Connected => "Connected".into(),
            Self::Reconnecting => "Reconnecting...".into(),
            Self::RateLimited => "Rate Limited".into(),
            Self::Error(msg) => format!("Error: {msg}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticsSnapshot {
    pub state: ConnectionState,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub rate_limit_hits: u64,
    pub retries: u64,
    pub last_latency_ms: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Default)]
pub struct NetworkDiagnostics {
    inner: Mutex<DiagnosticsSnapshot>,
}

impl NetworkDiagnostics {
    pub fn snapshot(&self) -> DiagnosticsSnapshot {
        self.inner.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn set_state(&self, state: ConnectionState) {
        if let Ok(mut g) = self.inner.lock() {
            if let ConnectionState::Error(ref msg) = state {
                g.last_error = Some(msg.clone());
            }
            g.state = state;
        }
    }

    pub fn record_success(&self, latency_ms: u64) {
        if let Ok(mut g) = self.inner.lock() {
            g.total_requests += 1;
            g.successful_requests += 1;
            g.last_latency_ms = Some(latency_ms);
            g.state = ConnectionState::Connected;
            g.last_error = None;
        }
    }

    pub fn record_rate_limit(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.rate_limit_hits += 1;
            g.state = ConnectionState::RateLimited;
        }
    }

    pub fn record_retry(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.retries += 1;
            g.state = ConnectionState::Reconnecting;
        }
    }

    pub fn record_failure(&self, err: impl Into<String>) {
        let msg = err.into();
        if let Ok(mut g) = self.inner.lock() {
            g.total_requests += 1;
            g.failed_requests += 1;
            g.last_error = Some(msg.clone());
            g.state = ConnectionState::Error(msg);
        }
    }
}
