use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use moq_lite::{Broadcast, Track};
use moq_native::ClientConfig;
use operational_transform::OperationSeq;
use tokio::sync::Mutex;
use url::Url;

/// Stress-test client for the MoQ OT server.
///
/// Maintains three copies of the document and compares them at the end:
///
/// 1. **local**  – built purely client-side by appending each payload right as
///    it is sent, no server involvement.
/// 2. **synced** – starts from the initial state obtained via the `state` track,
///    then each OT op echoed back on the `sync` track is composed in.
/// 3. **state**  – the authoritative full-text snapshot read from the `state`
///    track once all echoes have been received.

const PHASE_DURATION_SECS: u64 = 5;
/// (label, ops_per_second) for each burst phase
const PHASES: &[(&str, u64)] = &[("30 ops/s", 30), ("60 ops/s", 60), ("100 ops/s", 100)];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::WARN).init();

    let url = Url::parse("https://127.0.0.1:4443")?;
    let mut config = ClientConfig::default();
    config.tls.disable_verify = Some(true);

    let bc_name = "file1";
    let client_id = "stress";
    let own_bc_name = format!("{bc_name}/client/{client_id}");

    // -- origins ---------------------------------------------------------------
    let read_origin = moq_lite::Origin::produce();
    let publish_origin = moq_lite::Origin::produce();

    // -- our broadcast (update track) -----------------------------------------
    let mut client_bc = Broadcast::produce();
    let mut update_track = client_bc.producer.create_track(Track {
        name: "update".into(),
        priority: 1,
    });
    publish_origin
        .producer
        .publish_broadcast(&own_bc_name, client_bc.consumer);

    // -- connect ---------------------------------------------------------------
    let _session = moq_native::Client::new(config)?
        .with_consume(read_origin.producer.clone())
        .with_publish(publish_origin.consumer.clone())
        .connect(url)
        .await?;

    println!("Connected. Waiting for server broadcast '{bc_name}'…");

    // -- shared state ----------------------------------------------------------
    let sent_count = Arc::new(Mutex::new(0u64));
    let recv_count = Arc::new(Mutex::new(0u64));
    let burst_done = Arc::new(Mutex::new(false));

    // Copy 1: local — built right as we send
    let local_doc = Arc::new(Mutex::new(String::new()));

    // Copy 2: synced — initial state + composed sync ops
    // Stored as an OperationSeq so we can compose incoming ops onto it.
    let synced_doc = Arc::new(Mutex::new(OperationSeq::default()));
    // Flag: initial state has been captured into synced_doc
    let synced_initialised = Arc::new(Mutex::new(false));

    // Copy 3: state — latest full-text from the state track
    let state_doc = Arc::new(Mutex::new(String::new()));

    // The initial text the server uses (for seeding local_doc)
    let initial_text = Arc::new(Mutex::new(String::new()));

    // -- per-op timing log -----------------------------------------------------
    // Maps op counter -> (phase_index, sent_instant, Option<recv_instant>)
    let timing_log: Arc<Mutex<HashMap<u64, (usize, Instant, Option<Instant>)>>> =
        Arc::new(Mutex::new(HashMap::new()));
    // Record the absolute start so we can make times relative in the CSV
    let test_epoch = Instant::now();

    // -- subscribe task --------------------------------------------------------
    let own_bc_filter = own_bc_name.clone();
    let mut origin_consumer = read_origin.consumer;

    let recv_count_sub = recv_count.clone();
    let sent_count_sub = sent_count.clone();
    let burst_done_sub = burst_done.clone();
    let synced_doc_sub = synced_doc.clone();
    let synced_init_sub = synced_initialised.clone();
    let state_doc_sub = state_doc.clone();
    let initial_text_sub = initial_text.clone();
    let timing_log_sub = timing_log.clone();

    let subscribe_task = tokio::spawn(async move {
        while let Some((path, broadcast)) = origin_consumer.announced().await {
            let path_str = path.to_string();
            if path_str == own_bc_filter || path_str != bc_name {
                continue;
            }
            println!("Server broadcast found: [{path_str}]");

            let Some(broadcast) = broadcast else {
                continue;
            };

            // --- state track: read initial + keep updating state_doc ----------
            {
                let state_doc_inner = state_doc_sub.clone();
                let synced_doc_inner = synced_doc_sub.clone();
                let synced_init_inner = synced_init_sub.clone();
                let initial_text_inner = initial_text_sub.clone();
                let bc_state = broadcast.clone();
                tokio::spawn(async move {
                    let mut state_consumer =
                        bc_state.subscribe_track(&moq_lite::Track::new("state"));
                    println!("Subscribed to [{path_str}/state]");

                    while let Ok(Some(mut group)) = state_consumer.next_group().await {
                        while let Ok(Some(frame)) = group.read_frame().await {
                            let Ok(text) = String::from_utf8(frame.to_vec()) else {
                                continue;
                            };
                            // Packet: "version;content"
                            let Some((_ver, content)) = text.split_once(';') else {
                                continue;
                            };

                            // Always keep state_doc up to date
                            *state_doc_inner.lock().await = content.to_string();

                            // Re-seed synced_doc from state to prevent drift
                            {
                                let mut op = OperationSeq::default();
                                op.insert(content);
                                *synced_doc_inner.lock().await = op;
                            }

                            // First time: also seed initial_text
                            let mut init = synced_init_inner.lock().await;
                            if !*init {
                                *init = true;
                                *initial_text_inner.lock().await = content.to_string();
                                println!(
                                    "Initial state ({} chars): \"{}\"",
                                    content.len(),
                                    truncate(content, 80)
                                );
                            }
                        }
                    }
                });
            }

            // --- sync track: compose echoed ops into synced_doc ---------------
            {
                let recv_inner = recv_count_sub.clone();
                let burst_done_inner = burst_done_sub.clone();
                let sent_count_inner = sent_count_sub.clone();
                let synced_doc_inner = synced_doc_sub.clone();
                let timing_inner = timing_log_sub.clone();
                let client_id_owned = client_id.to_string();
                tokio::spawn(async move {
                    let mut sync_consumer =
                        broadcast.subscribe_track(&moq_lite::Track::new("sync"));
                    println!("Subscribed to [file1/sync]");

                    while let Ok(Some(mut group)) = sync_consumer.next_group().await {
                        while let Ok(Some(frame)) = group.read_frame().await {
                            let recv_at = Instant::now();
                            let Ok(text) = String::from_utf8(frame.to_vec()) else {
                                continue;
                            };
                            // Packet: "client_id;version;json_op"
                            let parts: Vec<&str> = text.splitn(3, ';').collect();
                            if parts.len() < 3 {
                                continue;
                            }
                            let from_client = parts[0];
                            if from_client != client_id_owned {
                                continue; // ignore other clients
                            }

                            let json_op = parts[2];

                            // Compose into synced_doc
                            if let Ok(incoming_op) = serde_json::from_str::<OperationSeq>(json_op) {
                                let mut doc = synced_doc_inner.lock().await;
                                match doc.compose(&incoming_op) {
                                    Ok(new_doc) => *doc = new_doc,
                                    Err(e) => eprintln!("  compose error: {e}"),
                                }
                            }

                            let op_num = {
                                let mut c = recv_inner.lock().await;
                                *c += 1;
                                *c
                            };

                            // Record receive timestamp
                            {
                                let mut log = timing_inner.lock().await;
                                if let Some(entry) = log.get_mut(&op_num) {
                                    entry.2 = Some(recv_at);
                                }
                            }

                            // If burst is done and all echoes received, we're done
                            let done = *burst_done_inner.lock().await;
                            if done {
                                let r = *recv_inner.lock().await;
                                let s = *sent_count_inner.lock().await;
                                if r >= s {
                                    println!("  All {r} echoes received.");
                                    return;
                                }
                            }
                        }
                    }
                });
            }
        }
    });

    // -- burst send task -------------------------------------------------------
    let local_doc_pub = local_doc.clone();
    let sent_count_pub = sent_count.clone();
    let burst_done_pub = burst_done.clone();
    let initial_text_pub = initial_text.clone();
    let synced_init_pub = synced_initialised.clone();
    let timing_log_pub = timing_log.clone();
    // Track doc length for building valid OT ops
    let doc_len: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

    let publish_task = tokio::spawn(async move {
        // Wait until we've captured the initial state
        println!("Waiting for initial state…");
        loop {
            if *synced_init_pub.lock().await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Seed local doc and doc_len from the initial state
        let init = initial_text_pub.lock().await.clone();
        *local_doc_pub.lock().await = init.clone();
        *doc_len.lock().await = Some(init.len() as u64);

        println!("\n========================================");
        println!("  STRESS TEST START");
        println!("  Phases:    {}", PHASES.len());
        for (i, (label, _)) in PHASES.iter().enumerate() {
            println!(
                "    Phase {}: {} for {}s",
                i + 1,
                label,
                PHASE_DURATION_SECS
            );
        }
        println!("========================================\n");

        let mut counter: u64 = 0;

        for (phase_idx, (label, ops_per_sec)) in PHASES.iter().enumerate() {
            let interval_ms = 1000 / ops_per_sec;
            println!(
                "  --- Phase {} ({}) — interval {}ms ---",
                phase_idx + 1,
                label,
                interval_ms
            );

            let phase_start = Instant::now();

            while phase_start.elapsed() < Duration::from_secs(PHASE_DURATION_SECS) {
                counter += 1;

                let payload = format!("[{counter}]");
                let payload_len = payload.len() as u64;

                // Build OT op: retain(current_len) + insert(payload)
                let current_len = doc_len.lock().await.unwrap();
                let mut op = OperationSeq::default();
                op.retain(current_len);
                op.insert(&payload);

                let json = serde_json::to_string(&op).unwrap();

                // Record send timestamp with phase index
                {
                    let mut log = timing_log_pub.lock().await;
                    log.insert(counter, (phase_idx, Instant::now(), None));
                }

                update_track.write_frame(bytes::Bytes::from(json));

                // Immediately append to local copy
                local_doc_pub.lock().await.push_str(&payload);
                *doc_len.lock().await = Some(current_len + payload_len);

                if counter % 50 == 0 {
                    println!(
                        "  sent {counter} ops ({:.1}s into phase)",
                        phase_start.elapsed().as_secs_f64()
                    );
                }

                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }

            let elapsed = phase_start.elapsed();
            let phase_ops = counter; // cumulative, but we print per-phase below
            println!(
                "  Phase {} done: {:.2}s elapsed, {} total ops so far",
                phase_idx + 1,
                elapsed.as_secs_f64(),
                phase_ops
            );
        }

        *sent_count_pub.lock().await = counter;
        *burst_done_pub.lock().await = true;

        println!("\n  All phases done: {counter} total ops sent.");
        println!("  Waiting for all echoes…\n");
    });

    // -- wait for everything ---------------------------------------------------
    let recv_count_wait = recv_count.clone();
    let sent_count_wait = sent_count.clone();

    let total_phase_secs = PHASE_DURATION_SECS * PHASES.len() as u64;
    let timeout = tokio::time::sleep(Duration::from_secs(total_phase_secs + 30));

    tokio::select! {
        _ = subscribe_task => {},
        _ = publish_task => {
            // Burst is done, wait for echoes to drain
            let drain_start = Instant::now();
            let drain_timeout = Duration::from_secs(20);
            loop {
                let r = *recv_count_wait.lock().await;
                let s = *sent_count_wait.lock().await;
                if s > 0 && r >= s {
                    break;
                }
                if drain_start.elapsed() > drain_timeout {
                    println!("  ⚠ Drain timeout after {drain_timeout:?}");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        },
        _ = timeout => {
            println!("  ⚠ Global timeout reached");
        },
        _ = tokio::signal::ctrl_c() => {
            println!("\n  Interrupted.");
        },
    }

    // Give the state track a moment to deliver the final snapshot
    tokio::time::sleep(Duration::from_secs(2)).await;

    // -- report ----------------------------------------------------------------
    let sent = *sent_count.lock().await;
    let recv = *recv_count.lock().await;

    let local = local_doc.lock().await.clone();
    let synced = synced_doc
        .lock()
        .await
        .apply("")
        .unwrap_or_else(|_| "<apply failed>".into());
    let state = state_doc.lock().await.clone();

    let ls = local == synced;
    let lt = local == state;
    let st = synced == state;

    // -- build report string (printed + saved) --------------------------------
    let mut report = String::new();
    report.push_str("\n========================================\n");
    report.push_str("  STRESS TEST RESULTS\n");
    report.push_str("========================================\n");
    report.push_str(&format!("  Ops sent:     {sent}\n"));
    report.push_str(&format!("  Ops echoed:   {recv}\n"));
    report.push_str(&format!(
        "  Loss:         {} ({:.1}%)\n",
        sent.saturating_sub(recv),
        if sent > 0 {
            (sent.saturating_sub(recv)) as f64 / sent as f64 * 100.0
        } else {
            0.0
        }
    ));
    report.push_str(&format!(
        "\n  LOCAL  ({:>6} chars): \"{}\"\n",
        local.len(),
        truncate(&local, 72)
    ));
    report.push_str(&format!(
        "  SYNCED ({:>6} chars): \"{}\"\n",
        synced.len(),
        truncate(&synced, 72)
    ));
    report.push_str(&format!(
        "  STATE  ({:>6} chars): \"{}\"\n",
        state.len(),
        truncate(&state, 72)
    ));
    report.push_str(&format!(
        "\n  local  == synced : {}\n",
        if ls { "OK" } else { "MISMATCH" }
    ));
    report.push_str(&format!(
        "  local  == state  : {}\n",
        if lt { "OK" } else { "MISMATCH" }
    ));
    report.push_str(&format!(
        "  synced == state  : {}\n",
        if st { "OK" } else { "MISMATCH" }
    ));

    if ls && lt {
        report.push_str("\n  All three copies match\n");
    } else {
        report.push_str("\n  MISMATCH detected\n");
        let pairs = [("local", &local), ("synced", &synced), ("state", &state)];
        for i in 0..pairs.len() {
            for j in (i + 1)..pairs.len() {
                let (na, a) = pairs[i];
                let (nb, b) = pairs[j];
                if a != b {
                    let pos = a
                        .chars()
                        .zip(b.chars())
                        .position(|(ca, cb)| ca != cb)
                        .unwrap_or(a.len().min(b.len()));
                    report.push_str(&format!("  {na} vs {nb}: first diff at char {pos}\n"));
                }
            }
        }
    }
    report.push_str("========================================\n");

    // Print to stdout
    print!("{report}");

    // -- write log file --------------------------------------------------------
    let log = timing_log.lock().await;
    let mut csv = String::new();
    let mut keys: Vec<u64> = log.keys().copied().collect();
    keys.sort();

    // Group ops by phase, write a header for each section
    let mut current_phase: Option<usize> = None;
    for op in keys {
        if let Some((phase, sent_at, recv_opt)) = log.get(&op) {
            if current_phase != Some(*phase) {
                current_phase = Some(*phase);
                let label = PHASES.get(*phase).map(|(l, _)| *l).unwrap_or("unknown");
                if *phase > 0 {
                    csv.push('\n');
                }
                csv.push_str(&format!("# Phase {} — {}\n", phase + 1, label));
                csv.push_str("op,sent_ms,recv_ms,rtt_ms\n");
            }
            let sent_ms = sent_at.duration_since(test_epoch).as_secs_f64() * 1000.0;
            match recv_opt {
                Some(recv_at) => {
                    let recv_ms = recv_at.duration_since(test_epoch).as_secs_f64() * 1000.0;
                    let rtt_ms = recv_at.duration_since(*sent_at).as_secs_f64() * 1000.0;
                    csv.push_str(&format!("{op},{sent_ms:.3},{recv_ms:.3},{rtt_ms:.3}\n"));
                }
                None => {
                    csv.push_str(&format!("{op},{sent_ms:.3},,\n"));
                }
            }
        }
    }

    csv.push_str("\n");
    csv.push_str(&report);
    csv.push_str("\n--- LOCAL DOC ---\n");
    csv.push_str(&local);
    csv.push_str("\n--- SYNCED DOC ---\n");
    csv.push_str(&synced);
    csv.push_str("\n--- STATE DOC ---\n");
    csv.push_str(&state);
    csv.push_str("\n");

    let log_path = "stress_log.csv";
    match std::fs::write(log_path, &csv) {
        Ok(_) => println!("\n  Log saved to {log_path}"),
        Err(e) => eprintln!("\n  Failed to write log: {e}"),
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max])
    } else {
        s.to_string()
    }
}
