//! Live load test statistics and circuit breaker.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const HISTORY_LEN: usize = 60;
const RESPONSE_LOG_LEN: usize = 30;
const BODY_PREVIEW_LEN: usize = 120;

/// Circuit breaker: after N consecutive 4xx/5xx, open for cooldown; then 1 probe.
pub const CIRCUIT_CONSECUTIVE_THRESHOLD: usize = 5;
/// Max cooldown (seconds). Fibonacci backoff capped at 377s.
const CIRCUIT_COOLDOWN_CAP_SECS: u64 = 377;

/// Nth Fibonacci number (1, 1, 2, 3, 5, 8, …), capped for use as seconds.
pub fn fib_secs(n: u32) -> u64 {
    let (mut a, mut b) = (1u64, 1u64);
    for _ in 0..n {
        let next = a.saturating_add(b);
        a = b;
        b = next;
    }
    a.min(CIRCUIT_COOLDOWN_CAP_SECS)
}

/// Circuit breaker state: closed (sending), open (cooldown), or half-open (probe).
#[derive(Debug, Clone, Copy, Default)]
pub enum CircuitState {
    /// Normal; requests are sent.
    #[default]
    Closed,
    /// Tripped; waiting until `open_until` before probing.
    Open { open_until: Instant },
    /// One probe allowed; waiting for its response.
    HalfOpen { probe_sent: bool },
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub state: CircuitState,
    pub consecutive_bad: usize,
    /// Number of times we've opened; used for Fibonacci backoff (1, 1, 2, 3, 5, 8… s).
    pub open_count: u32,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_bad: 0,
            open_count: 0,
        }
    }
}

impl CircuitBreaker {
    /// Returns true if we are allowed to send a request. May transition Open -> HalfOpen.
    pub fn can_send(&mut self, now: Instant) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open { open_until } => {
                if now >= open_until {
                    self.state = CircuitState::HalfOpen { probe_sent: false };
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen { probe_sent } => !probe_sent,
        }
    }

    /// Call after spawning the single probe request in HalfOpen.
    pub fn mark_probe_sent(&mut self) {
        if let CircuitState::HalfOpen { .. } = self.state {
            self.state = CircuitState::HalfOpen { probe_sent: true };
        }
    }

    /// Human-readable state and detail for UI. `now` for "waiting Xs".
    pub fn display(&self, now: Instant) -> (String, String) {
        match self.state {
            CircuitState::Closed => ("CLOSED".into(), "sending requests".into()),
            CircuitState::Open { open_until } => {
                let rem = if now >= open_until {
                    0
                } else {
                    open_until.duration_since(now).as_secs()
                };
                let fib = fib_secs(self.open_count);
                (
                    "OPEN".into(),
                    format!("waiting {}s (fib={}) before probe", rem, fib),
                )
            }
            CircuitState::HalfOpen { probe_sent } => {
                if probe_sent {
                    ("HALF-OPEN".into(), "waiting for probe response…".into())
                } else {
                    ("HALF-OPEN".into(), "sending 1 probe request…".into())
                }
            }
        }
    }
}

/// One entry in the response log (status + body preview).
#[derive(Debug, Clone)]
pub struct ResponseLogEntry {
    pub status: u16,
    pub latency_ms: u64,
    pub ok: bool,
    pub body_preview: String,
}

/// One row in the full request log (for export).
#[derive(Debug, Clone)]
pub struct RequestRecord {
    pub seq: u64,
    pub elapsed_ms: u64,
    pub status: u16,
    pub latency_ms: u64,
    pub ok: bool,
    pub body_preview: String,
}

/// Frozen snapshot of test results for the report screen.
#[derive(Debug, Clone)]
pub struct ReportData {
    pub total: u64,
    pub ok: u64,
    pub err: u64,
    pub p50: u64,
    pub p95: u64,
    pub p99: u64,
    pub min_latency: u64,
    pub max_latency: u64,
    pub avg_latency: f64,
    pub rps: u64,
    pub success_rate: f64,
    pub elapsed: Duration,
    pub status_codes: HashMap<u16, u64>,
    pub url: String,
    pub method: String,
    pub requests: Vec<RequestRecord>,
}

/// Live stats: counts, latencies, RPS history, response log, circuit breaker.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub total: u64,
    pub ok: u64,
    pub err: u64,
    pub latencies_ms: Vec<u64>,
    pub rps_history: VecDeque<u64>,
    pub last_rps_tick: Option<Instant>,
    pub rps_accum: u64,
    pub response_log: VecDeque<ResponseLogEntry>,
    pub circuit: CircuitBreaker,
    pub status_codes: HashMap<u16, u64>,
    pub start_time: Option<Instant>,
    pub test_elapsed: Duration,
    pub min_latency: u64,
    pub max_latency: u64,
    pub request_log: Vec<RequestRecord>,
}

impl Stats {
    pub fn record(&mut self, ok: bool, latency_ms: u64, status: u16, body_preview: String) {
        let now = Instant::now();
        if self.start_time.is_none() {
            self.start_time = Some(now);
        }
        let bad = status >= 400 || !ok;
        *self.status_codes.entry(status).or_insert(0) += 1;
        if self.total == 0 {
            self.min_latency = latency_ms;
            self.max_latency = latency_ms;
        } else {
            self.min_latency = self.min_latency.min(latency_ms);
            self.max_latency = self.max_latency.max(latency_ms);
        }
        if bad {
            self.circuit.consecutive_bad += 1;
        } else {
            self.circuit.consecutive_bad = 0;
        }

        match self.circuit.state {
            CircuitState::HalfOpen { .. } => {
                if ok {
                    self.circuit.state = CircuitState::Closed;
                    self.circuit.consecutive_bad = 0;
                    self.circuit.open_count = 0;
                } else {
                    let secs = fib_secs(self.circuit.open_count);
                    self.circuit.open_count = self.circuit.open_count.saturating_add(1);
                    self.circuit.state = CircuitState::Open {
                        open_until: now + Duration::from_secs(secs),
                    };
                }
            }
            CircuitState::Closed => {
                if self.circuit.consecutive_bad >= CIRCUIT_CONSECUTIVE_THRESHOLD {
                    let secs = fib_secs(self.circuit.open_count);
                    self.circuit.open_count = self.circuit.open_count.saturating_add(1);
                    self.circuit.state = CircuitState::Open {
                        open_until: now + Duration::from_secs(secs),
                    };
                }
            }
            CircuitState::Open { .. } => {}
        }

        self.total += 1;
        if ok {
            self.ok += 1;
        } else {
            self.err += 1;
        }
        self.latencies_ms.push(latency_ms);
        if self.latencies_ms.len() > 10_000 {
            self.latencies_ms.drain(..5000);
        }
        self.rps_accum += 1;
        let preview = if body_preview.len() > BODY_PREVIEW_LEN {
            format!("{}…", &body_preview[..BODY_PREVIEW_LEN])
        } else {
            body_preview
        };
        let elapsed_ms = self
            .start_time
            .map(|s| now.duration_since(s).as_millis() as u64)
            .unwrap_or(0);
        self.response_log.push_back(ResponseLogEntry {
            status,
            latency_ms,
            ok,
            body_preview: preview.clone(),
        });
        if self.response_log.len() > RESPONSE_LOG_LEN {
            self.response_log.pop_front();
        }
        self.request_log.push(RequestRecord {
            seq: self.total,
            elapsed_ms,
            status,
            latency_ms,
            ok,
            body_preview: preview,
        });
    }

    pub fn tick_rps(&mut self) {
        let now = Instant::now();
        if let Some(start) = self.start_time {
            self.test_elapsed = now.duration_since(start);
        }
        if let Some(prev) = self.last_rps_tick {
            if now.duration_since(prev) >= Duration::from_secs(1) {
                self.rps_history.push_back(self.rps_accum);
                self.rps_accum = 0;
                self.last_rps_tick = Some(now);
                if self.rps_history.len() > HISTORY_LEN {
                    self.rps_history.pop_front();
                }
            }
        } else {
            self.last_rps_tick = Some(now);
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            self.ok as f64 / self.total as f64
        }
    }

    pub fn rps(&self) -> u64 {
        self.rps_history.back().copied().unwrap_or(0)
    }

    pub fn percentile(&self, p: f64) -> u64 {
        if self.latencies_ms.is_empty() {
            return 0;
        }
        let mut sorted = self.latencies_ms.clone();
        sorted.sort_unstable();
        let idx = ((p / 100.0) * sorted.len() as f64) as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    pub fn p50(&self) -> u64 {
        self.percentile(50.0)
    }
    pub fn p95(&self) -> u64 {
        self.percentile(95.0)
    }
    pub fn p99(&self) -> u64 {
        self.percentile(99.0)
    }

    pub fn sparkline_data(&self) -> Vec<u64> {
        self.rps_history.iter().copied().collect()
    }

    pub fn snapshot(&self, url: &str, method: &str) -> ReportData {
        let avg = if self.latencies_ms.is_empty() {
            0.0
        } else {
            self.latencies_ms.iter().sum::<u64>() as f64 / self.latencies_ms.len() as f64
        };
        ReportData {
            total: self.total,
            ok: self.ok,
            err: self.err,
            p50: self.p50(),
            p95: self.p95(),
            p99: self.p99(),
            min_latency: self.min_latency,
            max_latency: self.max_latency,
            avg_latency: avg,
            rps: self.rps(),
            success_rate: self.success_rate(),
            elapsed: self.test_elapsed,
            status_codes: self.status_codes.clone(),
            url: url.to_string(),
            method: method.to_string(),
            requests: self.request_log.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_empty() {
        let s = Stats::default();
        assert_eq!(s.percentile(50.0), 0);
        assert_eq!(s.p50(), 0);
    }

    #[test]
    fn percentile_and_p50_p99() {
        let s = Stats {
            latencies_ms: vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
            ..Default::default()
        };
        assert_eq!(s.p50(), 60); // (50/100)*10 -> index 5
        assert_eq!(s.p95(), 100); // index 9
        assert_eq!(s.p99(), 100); // index 9
    }

    #[test]
    fn circuit_opens_after_threshold() {
        let mut s = Stats::default();
        for _ in 0..CIRCUIT_CONSECUTIVE_THRESHOLD {
            s.record(false, 0, 500, String::new());
        }
        assert!(matches!(s.circuit.state, CircuitState::Open { .. }));
    }

    #[test]
    fn success_rate() {
        let mut s = Stats::default();
        assert_eq!(s.success_rate(), 1.0);
        s.total = 10;
        s.ok = 8;
        s.err = 2;
        assert!((s.success_rate() - 0.8).abs() < 1e-9);
    }

    #[test]
    fn tick_rps_caps_history() {
        let mut s = Stats::default();
        for _ in 0..100 {
            s.rps_accum += 1;
            s.last_rps_tick = Some(Instant::now());
            s.tick_rps();
        }
        assert!(s.rps_history.len() <= 60);
    }
}
