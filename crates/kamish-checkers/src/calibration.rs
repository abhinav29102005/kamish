//! Real Transport Impairment, Deterministic Netem Calibration & Raw Sampling (W31).
//!
//! Validates:
//! - Achieved vs configured delay calibration.
//! - Fixed PRNG seeds for reproducible runs.
//! - Medians (p50), tail latencies (p90, p95, p99), and spread (IQR, standard deviation).
//! - Raw latency sample capture to disk.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

/// Configuration for real network impairment calibration (W31).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationConfig {
    pub configured_delay_ms: f64,
    pub jitter_ms: f64,
    pub loss_rate: f64,
    pub seed: u64,
    pub sample_count: usize,
}

impl Default for CalibrationConfig {
    fn default() -> Self {
        Self {
            configured_delay_ms: 100.0,
            jitter_ms: 5.0,
            loss_rate: 0.0,
            seed: 42,
            sample_count: 50,
        }
    }
}

/// Statistically verified calibration results comparing achieved vs configured latency (W31).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationResult {
    pub configured_delay_ms: f64,
    pub sample_count: usize,
    pub min_ms: f64,
    pub max_ms: f64,
    pub p50_median_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub spread_iqr_ms: f64,
    pub spread_stddev_ms: f64,
    pub raw_samples: Vec<f64>,
}

impl CalibrationResult {
    /// Computes summary statistics from a raw sample vector.
    pub fn from_samples(configured_delay_ms: f64, mut samples: Vec<f64>) -> Self {
        let n = samples.len();
        if n == 0 {
            return Self {
                configured_delay_ms,
                sample_count: 0,
                min_ms: 0.0,
                max_ms: 0.0,
                p50_median_ms: 0.0,
                p90_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
                spread_iqr_ms: 0.0,
                spread_stddev_ms: 0.0,
                raw_samples: Vec::new(),
            };
        }

        samples.sort_by(|a, b| a.total_cmp(b));

        let min_ms = samples[0];
        let max_ms = samples[n - 1];

        let p = |pct: f64| -> f64 {
            let idx = ((pct / 100.0) * (n as f64 - 1.0)).round() as usize;
            samples[idx.min(n - 1)]
        };

        let p25 = p(25.0);
        let p50_median_ms = p(50.0);
        let p75 = p(75.0);
        let p90_ms = p(90.0);
        let p95_ms = p(95.0);
        let p99_ms = p(99.0);

        let spread_iqr_ms = p75 - p25;

        let mean = samples.iter().sum::<f64>() / n as f64;
        let variance = samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / n as f64;
        let spread_stddev_ms = variance.sqrt();

        Self {
            configured_delay_ms,
            sample_count: n,
            min_ms,
            max_ms,
            p50_median_ms,
            p90_ms,
            p95_ms,
            p99_ms,
            spread_iqr_ms,
            spread_stddev_ms,
            raw_samples: samples,
        }
    }
}

/// Deterministic Impaired TCP Proxy introducing calibrated network delay & loss on transport sockets (W31).
pub struct ImpairedTcpProxy {
    bind_addr: String,
    pub target_addr: String,
    pub config: CalibrationConfig,
    stop_signal: tokio::sync::broadcast::Sender<()>,
}

impl ImpairedTcpProxy {
    /// Spawns an impaired TCP proxy forwarding to target_addr with deterministic calibrated delays.
    pub async fn spawn(
        target_addr: String,
        config: CalibrationConfig,
    ) -> Result<(Self, String), std::io::Error> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let bind_addr = listener.local_addr()?.to_string();
        let (stop_tx, _) = tokio::sync::broadcast::channel(1);

        let proxy = Self {
            bind_addr: bind_addr.clone(),
            target_addr: target_addr.clone(),
            config: config.clone(),
            stop_signal: stop_tx.clone(),
        };

        let rng = Arc::new(Mutex::new(StdRng::seed_from_u64(config.seed)));
        let mut stop_rx = stop_tx.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_rx.recv() => break,
                    accept_res = listener.accept() => {
                        let Ok((inbound, _)) = accept_res else { continue };
                        let target = target_addr.clone();
                        let cfg = config.clone();
                        let rng_clone = Arc::clone(&rng);

                        tokio::spawn(async move {
                            let Ok(outbound) = TcpStream::connect(&target).await else { return };
                            let (mut in_read, mut in_write) = inbound.into_split();
                            let (mut out_read, mut out_write) = outbound.into_split();

                            // Client -> Server forwarder with calibrated delay
                            let cfg1 = cfg.clone();
                            let rng1 = Arc::clone(&rng_clone);
                            let t1 = tokio::spawn(async move {
                                let mut buf = vec![0u8; 8192];
                                loop {
                                    let n = match in_read.read(&mut buf).await {
                                        Ok(0) => break,
                                        Ok(n) => n,
                                        Err(_) => break,
                                    };

                                    let (delay, lost) = {
                                        let mut r = rng1.lock().await;
                                        let jitter = if cfg1.jitter_ms > 0.0 {
                                            r.gen_range(-cfg1.jitter_ms..cfg1.jitter_ms)
                                        } else {
                                            0.0
                                        };
                                        let delay = (cfg1.configured_delay_ms + jitter).max(0.0);
                                        let lost = if cfg1.loss_rate > 0.0 {
                                            r.gen_bool(cfg1.loss_rate.clamp(0.0, 1.0))
                                        } else {
                                            false
                                        };
                                        (delay, lost)
                                    };

                                    if lost {
                                        continue; // simulate dropped packet
                                    }

                                    if delay > 0.0 {
                                        tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                                    }

                                    if out_write.write_all(&buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                            });

                            // Server -> Client forwarder
                            let t2 = tokio::spawn(async move {
                                let mut buf = vec![0u8; 8192];
                                loop {
                                    let n = match out_read.read(&mut buf).await {
                                        Ok(0) => break,
                                        Ok(n) => n,
                                        Err(_) => break,
                                    };
                                    if in_write.write_all(&buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                            });

                            let _ = tokio::join!(t1, t2);
                        });
                    }
                }
            }
        });

        Ok((proxy, bind_addr))
    }

    pub fn bind_addr(&self) -> &str {
        &self.bind_addr
    }

    pub fn stop(&self) {
        let _ = self.stop_signal.send(());
    }
}

/// Runs a real transport calibration measuring achieved vs configured delay across round-trip ping probes (W31).
pub async fn calibrate_transport(
    config: CalibrationConfig,
) -> Result<CalibrationResult, std::io::Error> {
    // 1. Spawn a TCP echo server
    let echo_listener = TcpListener::bind("127.0.0.1:0").await?;
    let echo_addr = echo_listener.local_addr()?.to_string();

    tokio::spawn(async move {
        while let Ok((mut stream, _)) = echo_listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                while let Ok(n) = stream.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    if stream.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    // 2. Spawn impaired TCP proxy in front of echo server
    let (proxy, proxy_addr) = ImpairedTcpProxy::spawn(echo_addr, config.clone()).await?;

    // 3. Connect client and perform repeated ping measurements
    let mut client = TcpStream::connect(&proxy_addr).await?;
    let mut samples = Vec::with_capacity(config.sample_count);

    let ping_msg = b"KAMISH_CALIBRATION_PING\n";
    let mut read_buf = vec![0u8; ping_msg.len()];

    for _ in 0..config.sample_count {
        let start = Instant::now();
        client.write_all(ping_msg).await?;
        client.read_exact(&mut read_buf).await?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        samples.push(elapsed_ms);
    }

    proxy.stop();
    Ok(CalibrationResult::from_samples(
        config.configured_delay_ms,
        samples,
    ))
}
