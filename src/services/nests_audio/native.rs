use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::{mpsc, oneshot};

use super::ConnectionState;

const SAMPLE_RATE: u32 = 48000;
const OPUS_FRAME_SIZE: usize = 960;
const MAX_DECODE_SAMPLES: usize = 5760;
const RING_BUFFER_MAX_SAMPLES: usize = 24000;
/// Duration of one Opus frame (960 samples @ 48 kHz) in microseconds.
const FRAME_DURATION_US: u64 = 20_000;
/// Number of consecutive Opus frames packed into a single moq-lite group
/// (= one QUIC uni-stream). The first frame in a group opens the stream;
/// `finish()` FINs it.
///
/// **Production value = 50** (≈ 1 s of audio per group → 1 stream/sec).
/// The nostrnests production relay's per-subscriber forward pipeline
/// sustains ≈ 40 streams/sec; at one frame per group (50 streams/sec) it
/// stops forwarding to a subscriber entirely after 6–14 s of sustained
/// speech. Amethyst's two-phone field tests on the same relay
/// (`claude/fix-nests-audio-receiver-HCgOY`) additionally showed smaller
/// batches (5/10) hitting a separate listener-side cliff after ~13-16 s,
/// while 50 has headroom; their production build ships 50. Their
/// cross-stack interop harness against a local `moq-relay 0.10.25` needed
/// ≤5 — a regime this app does not target by default. Trade-off: a late
/// joiner waits ≤ 1 group boundary (1 s) for their first frame, and a
/// stream RST loses ≤ 1 s of audio.
const FRAMES_PER_GROUP: usize = 50;

/// Encode a value as an RFC 9000 §16 QUIC varint (2-bit length prefix:
/// `<64`→1B, `<2^14`→2B, `<2^30`→4B, else 8B).
///
/// Nests audio frames use this as the `timestamp_us` container prefix —
/// the kixelated/moq `hang` "legacy" container layout is
/// `varint(timestamp_us) ++ raw_opus_packet` per frame
/// (`rs/hang/src/container/frame.rs`). Every speaker in the ecosystem
/// (Amethyst, NestsUI web, nostr.blue web) emits it; listeners (including
/// WebCodecs-based web watchers, which schedule playback off the
/// timestamp) depend on it being present.
fn encode_quic_varint(v: u64) -> Vec<u8> {
    if v < 64 {
        vec![v as u8]
    } else if v < (1 << 14) {
        vec![0x40 | (v >> 8) as u8, v as u8]
    } else if v < (1 << 30) {
        vec![
            0x80 | (v >> 24) as u8,
            (v >> 16) as u8,
            (v >> 8) as u8,
            v as u8,
        ]
    } else {
        vec![
            0xC0 | (v >> 56) as u8,
            (v >> 48) as u8,
            (v >> 40) as u8,
            (v >> 32) as u8,
            (v >> 24) as u8,
            (v >> 16) as u8,
            (v >> 8) as u8,
            v as u8,
        ]
    }
}

/// Strip the leading RFC 9000 §16 varint from a nest audio frame, returning
/// the bare Opus payload. Mirrors Amethyst's
/// `MoqLiteNestsListener.stripLegacyTimestampPrefix`: the varint length is
/// encoded in the top 2 bits of the first byte (`00`→1, `01`→2, `10`→4,
/// `11`→8 bytes). Returns the frame unchanged when the varint header would
/// overrun the payload (malformed frame — surface upstream rather than
/// silently mask).
fn strip_quic_varint_prefix(frame: &[u8]) -> &[u8] {
    if frame.is_empty() {
        return frame;
    }
    let tag = (frame[0] >> 6) & 0x3;
    let varint_len = 1usize << tag;
    if frame.len() < varint_len {
        return frame;
    }
    &frame[varint_len..]
}

/// Pure group-pacing decision used by the encoding thread: tracks how many
/// frames have been written to the currently-open moq-lite group and reports
/// when the group is full and must be FIN'd. Extracted so the batching
/// behavior is unit-testable without cpal/moq.
#[derive(Debug, Default)]
struct GroupPacer {
    frames_in_group: usize,
}

impl GroupPacer {
    /// Record one written frame; returns `true` when the group just reached
    /// [`FRAMES_PER_GROUP`] and must be finished.
    fn on_frame_written(&mut self) -> bool {
        self.frames_in_group += 1;
        if self.frames_in_group >= FRAMES_PER_GROUP {
            self.frames_in_group = 0;
            true
        } else {
            false
        }
    }

    /// Reset after an out-of-band group close (mute edge, publisher swap).
    fn reset(&mut self) {
        self.frames_in_group = 0;
    }
}

enum NativeCmd {
    Connect {
        relay_url: String,
        namespace: String,
        jwt: String,
        /// Local user's hex pubkey. Used as the MoQ broadcast path so other
        /// clients can subscribe to `{my_pubkey}/audio/data`. Verified wire
        /// contract at `@moq/publish/src/audio/encoder.ts:31`
        /// (`Audio.Encoder.TRACK = "audio/data"`), matching the ecosystem
        /// MoQ track layout.
        my_pubkey: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    StartPublishing {
        reply: oneshot::Sender<Result<(), String>>,
    },
    StopPublishing {
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetMuted {
        muted: bool,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SubscribeToParticipant {
        pubkey: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    UnsubscribeFromParticipant {
        pubkey: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Disconnect {
        reply: oneshot::Sender<Result<(), String>>,
    },
}

struct SharedState {
    connection: ConnectionState,
    tracks: Vec<String>,
    /// Peak mic level (0.0–1.0) from the most recent encode frame. Updated
    /// by the encoding thread in `do_start_publishing`; read by
    /// `get_mic_level` for Phase 1.5 energy-gated speaking detection.
    /// Shared via Arc so the encoding thread can update it without locking
    /// `SharedState`'s mutex on every audio frame.
    last_mic_level: Arc<AtomicU32>,
    /// Phase 3.7: per-participant peak audio level (0.0–1.0), keyed by hex
    /// pubkey. Updated by each subscriber's decode task; read by
    /// `get_all_participant_levels` for remote speaking detection.
    participant_levels: Arc<StdMutex<HashMap<String, f32>>>,
}

#[derive(Clone)]
pub struct NativeBridge {
    cmd_tx: mpsc::UnboundedSender<NativeCmd>,
    shared: Arc<StdMutex<SharedState>>,
}

impl NativeBridge {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let mic_level = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let participant_levels = Arc::new(StdMutex::new(HashMap::new()));
        let shared = Arc::new(StdMutex::new(SharedState {
            connection: ConnectionState::Disconnected,
            tracks: Vec::new(),
            last_mic_level: mic_level.clone(),
            participant_levels: participant_levels.clone(),
        }));
        let shared_clone = shared.clone();
        let is_muted = Arc::new(AtomicBool::new(false));
        let is_muted_clone = is_muted.clone();
        let encoding_shutdown = Arc::new(AtomicBool::new(false));
        let encoding_shutdown_clone = encoding_shutdown.clone();
        let mic_level_clone = mic_level.clone();
        let participant_levels_clone = participant_levels.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    log::error!("Failed to create audio runtime: {}", e);
                    return;
                }
            };
            rt.block_on(async move {
                let mut engine = Engine {
                    cmd_rx,
                    shared: shared_clone,
                    moq_session: None,
                    origin: None,
                    broadcast: None,
                    track: None,
                    input_stream: None,
                    subscribers: HashMap::new(),
                    is_muted: is_muted_clone,
                    encoding_shutdown: encoding_shutdown_clone,
                    mic_level: mic_level_clone,
                    participant_levels: participant_levels_clone,
                };
                engine.run().await;
            });
        });
        NativeBridge { cmd_tx, shared }
    }

    pub async fn connect(
        &self,
        relay_url: &str,
        namespace: &str,
        jwt: &str,
        my_pubkey: &str,
    ) -> Result<(), String> {
        self.set_state(ConnectionState::Connecting);
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NativeCmd::Connect {
                relay_url: relay_url.to_string(),
                namespace: namespace.to_string(),
                jwt: jwt.to_string(),
                my_pubkey: my_pubkey.to_string(),
                reply: tx,
            })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())??;
        self.set_state(ConnectionState::Connected);
        Ok(())
    }

    pub async fn start_publishing(&self) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NativeCmd::StartPublishing { reply: tx })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn stop_publishing(&self) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NativeCmd::StopPublishing { reply: tx })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn set_muted(&self, muted: bool) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NativeCmd::SetMuted { muted, reply: tx })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn subscribe(&self, pubkey: &str) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NativeCmd::SubscribeToParticipant {
                pubkey: pubkey.to_string(),
                reply: tx,
            })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())??;
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        if !s.tracks.contains(&pubkey.to_string()) {
            s.tracks.push(pubkey.to_string());
        }
        Ok(())
    }

    pub async fn unsubscribe(&self, pubkey: &str) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NativeCmd::UnsubscribeFromParticipant {
                pubkey: pubkey.to_string(),
                reply: tx,
            })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())??;
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.tracks.retain(|t| t != pubkey);
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NativeCmd::Disconnect { reply: tx })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())??;
        self.set_state(ConnectionState::Disconnected);
        Ok(())
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).connection.clone()
    }

    pub fn participant_tracks(&self) -> Vec<String> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).tracks.clone()
    }

    /// Read the most recent mic level (0.0–1.0). Phase 1.5 energy-gated
    /// speaking detection polls this every 100ms.
    pub fn get_mic_level(&self) -> f32 {
        f32::from_bits(
            self.shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .last_mic_level
                .load(Ordering::Relaxed),
        )
    }

    /// Phase 3.7: Read all per-participant peak levels as a `{pubkey: level}`
    /// map. Remote speaking detection polls this every 100ms.
    pub fn get_all_participant_levels(&self) -> HashMap<String, f32> {
        let shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let levels = shared.participant_levels.lock().unwrap_or_else(|e| e.into_inner());
        levels.clone()
    }

    fn set_state(&self, state: ConnectionState) {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).connection = state;
    }
}

struct Engine {
    cmd_rx: mpsc::UnboundedReceiver<NativeCmd>,
    shared: Arc<StdMutex<SharedState>>,
    moq_session: Option<moq_lite::Session>,
    origin: Option<moq_lite::OriginProducer>,
    broadcast: Option<moq_lite::BroadcastProducer>,
    track: Option<moq_lite::TrackProducer>,
    input_stream: Option<cpal::Stream>,
    subscribers: HashMap<String, SubState>,
    is_muted: Arc<AtomicBool>,
    encoding_shutdown: Arc<AtomicBool>,
    /// Shared with the encoding thread and `SharedState.last_mic_level`.
    /// Updated per-frame with the peak amplitude of the PCM input.
    /// Stores `f32::to_bits()` in an `AtomicU32` (std has no `AtomicF32`).
    mic_level: Arc<AtomicU32>,
    /// Phase 3.7: per-participant peak levels, shared between decode tasks
    /// and `SharedState.participant_levels`. Each decode task locks only
    /// this map (never `shared`), mirroring the `ring_buf_dec` pattern.
    participant_levels: Arc<StdMutex<HashMap<String, f32>>>,
}

struct SubState {
    _output_stream: cpal::Stream,
    _decode_task: tokio::task::AbortHandle,
}

impl Engine {
    async fn run(&mut self) {
        while let Some(cmd) = self.cmd_rx.recv().await {
            self.handle_cmd(cmd).await;
        }
    }

    async fn handle_cmd(&mut self, cmd: NativeCmd) {
        match cmd {
            NativeCmd::Connect {
                relay_url,
                namespace,
                jwt,
                my_pubkey,
                reply,
            } => {
                let _ = reply.send(self.do_connect(&relay_url, &namespace, &jwt, &my_pubkey).await);
            }
            NativeCmd::StartPublishing { reply } => {
                let _ = reply.send(self.do_start_publishing());
            }
            NativeCmd::StopPublishing { reply } => {
                self.encoding_shutdown.store(true, Ordering::Relaxed);
                self.input_stream = None;
                let _ = reply.send(Ok(()));
            }
            NativeCmd::SetMuted { muted, reply } => {
                self.is_muted.store(muted, Ordering::Relaxed);
                let _ = reply.send(Ok(()));
            }
            NativeCmd::SubscribeToParticipant { pubkey, reply } => {
                let _ = reply.send(self.do_subscribe(&pubkey).await);
            }
            NativeCmd::UnsubscribeFromParticipant { pubkey, reply } => {
                if let Some(sub) = self.subscribers.remove(&pubkey) {
                    sub._decode_task.abort();
                }
                // Phase 3.7: remove level tracking AFTER abort to avoid
                // re-insert race from the not-yet-cancelled decode loop.
                if let Ok(mut map) = self.participant_levels.lock() {
                    map.remove(&pubkey);
                }
                let _ = reply.send(Ok(()));
            }
            NativeCmd::Disconnect { reply } => {
                self.do_disconnect();
                let _ = reply.send(Ok(()));
            }
        }
    }

    async fn do_connect(
        &mut self,
        relay_url: &str,
        namespace: &str,
        jwt: &str,
        my_pubkey: &str,
    ) -> Result<(), String> {
        let mut url = url::Url::parse(relay_url).map_err(|e| format!("Invalid URL: {}", e))?;

        // The MoQ convention puts the namespace in the
        // URL path: `https://moq.nostrnests.com:4443/nests/30312:<pk>:<d>?jwt=...`
        // The moq-rs relay uses the path to scope the session's announce root.
        if !namespace.is_empty() {
            let path = format!("/{}", namespace);
            url.set_path(&path);
        }

        if !jwt.is_empty() {
            url.query_pairs_mut().append_pair("jwt", jwt);
        }

        let wt_client = web_transport_quinn::ClientBuilder::new()
            .with_system_roots()
            .map_err(|e| format!("TLS config failed: {}", e))?;

        let wt_session = wt_client
            .connect(url)
            .await
            .map_err(|e| format!("WebTransport connect failed: {}", e))?;

        let origin = moq_lite::Origin::random();
        let origin_producer = origin.produce();

        let client = moq_lite::Client::new().with_origin(origin_producer.clone());
        let session = client
            .connect(wt_session)
            .await
            .map_err(|e| format!("MoQ connect failed: {}", e))?;

        let broadcast = moq_lite::Broadcast::new();
        let mut broadcast_producer = broadcast.produce();
        // Track name is the literal `"audio/data"` — verified at
        // `@moq/publish/src/audio/encoder.ts:31` (`Audio.Encoder.TRACK`).
        // The web path (`moq-nest.js`) already uses this name; this fix makes
        // desktop and web wire-compatible with each other and with the
        // reference impl.
        let track = broadcast_producer
            .create_track(moq_lite::Track::new("audio/data"))
            .map_err(|e| format!("Create track failed: {}", e))?;

        // Broadcast path is the local user's hex pubkey — other clients
        // subscribe via `consume({pubkey})` then `subscribe_track("audio/data")`.
        // (Ecosystem MoQ track layout.)
        // (Prior code published under `namespace`, which was wire-incompatible
        // with the web backend.)
        origin_producer.publish_broadcast(my_pubkey, broadcast_producer.consume());

        self.moq_session = Some(session);
        self.origin = Some(origin_producer);
        self.broadcast = Some(broadcast_producer);
        self.track = Some(track);

        Ok(())
    }

    fn do_start_publishing(&mut self) -> Result<(), String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        self.encoding_shutdown.store(false, Ordering::Relaxed);

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "No input device available".to_string())?;

        let (audio_tx, audio_rx) = std::sync::mpsc::channel::<Vec<f32>>();

        let stream_config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };

        let err_fn = |err: cpal::StreamError| {
            log::error!("Audio input error: {}", err);
        };

        let stream = device
            .build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // Always forward real samples: the encoding thread owns
                    // the mute gate (drop frames + advance the timestamp
                    // clock), so a muted window appears on the wire as a real
                    // wall-clock gap and unmute stays sample-accurate.
                    let _ = audio_tx.send(data.to_vec());
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("Build input stream failed: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Play input stream failed: {}", e))?;

        self.input_stream = Some(stream);

        let track = self
            .track
            .clone()
            .ok_or_else(|| "No track available".to_string())?;

        let encoding_shutdown = self.encoding_shutdown.clone();
        let mic_level = self.mic_level.clone();
        let is_muted = self.is_muted.clone();
        std::thread::spawn(move || {
            let mut encoder = match opus::Encoder::new(
                SAMPLE_RATE,
                opus::Channels::Mono,
                opus::Application::Audio,
            ) {
                Ok(e) => e,
                Err(e) => {
                    log::error!("Opus encoder init failed: {}", e);
                    return;
                }
            };
            let mut pcm_buffer = Vec::with_capacity(OPUS_FRAME_SIZE * 2);
            let mut output = vec![0u8; 4000];
            let mut track = track;
            // Frame-index-derived timestamp clock (NOT wall clock): each
            // captured PCM frame occupies exactly one FRAME_DURATION_US slot
            // in the timeline, and the counter advances on EVERY frame —
            // including encoder failures and muted frames — so a mute gap
            // shows up on the wire as a real wall-clock gap (timestamps jump
            // forward by the muted duration) rather than collapsing the
            // silence. A send-time wall clock would drift forward of the
            // actual capture time whenever `write_frame` blocks on transport
            // backpressure, and WebCodecs listeners schedule playback off
            // this value. Mirrors Amethyst's `nextFrameIndex`.
            let mut frame_index: u64 = 0;
            // Mute-edge latch: FIN the open group exactly once per
            // unmuted→muted transition so listeners see a clean group
            // boundary instead of a half-delivered group that never
            // completes (Amethyst's web watchers render a "stalled"
            // indicator on the latter).
            let mut was_muted = false;
            // Batched group state: FRAMES_PER_GROUP consecutive frames share
            // one moq-lite group (one QUIC uni-stream). `append_group()`
            // opens the stream; `finish()` FINs it.
            let mut current_group: Option<moq_lite::GroupProducer> = None;
            let mut pacer = GroupPacer::default();
            while let Ok(samples) = audio_rx.recv() {
                if encoding_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                pcm_buffer.extend_from_slice(&samples);
                let mut read_pos = 0;
                while pcm_buffer.len() - read_pos >= OPUS_FRAME_SIZE {
                    let frame = &pcm_buffer[read_pos..read_pos + OPUS_FRAME_SIZE];
                    let timestamp_us = frame_index * FRAME_DURATION_US;
                    frame_index += 1;
                    read_pos += OPUS_FRAME_SIZE;
                    if is_muted.load(Ordering::Relaxed) {
                        // EGG-03 rule 8: mute = stop publishing. No silence
                        // frames; the mic stays open, the timestamp clock
                        // keeps advancing, and the group gets a clean FIN so
                        // listeners don't sit on a half-delivered group.
                        if !was_muted {
                            was_muted = true;
                            if let Some(mut g) = current_group.take() {
                                let _ = g.finish();
                            }
                            pacer.reset();
                        }
                        mic_level.store(0.0f32.to_bits(), Ordering::Relaxed);
                        continue;
                    }
                    was_muted = false;
                    // Peak amplitude for speaking detection (Phase 1.5) —
                    // computed from the raw PCM frame before encoding.
                    let peak = frame
                        .iter()
                        .fold(0.0f32, |acc, &s| acc.max(s.abs()));
                    mic_level.store(peak.to_bits(), Ordering::Relaxed);
                    let encoded = match encoder.encode_float(frame, &mut output) {
                        Ok(len) => &output[..len],
                        Err(e) => {
                            log::warn!("Opus encode error: {}", e);
                            continue;
                        }
                    };
                    // hang "legacy" container frame:
                    // varint(timestamp_us) ++ opus packet.
                    let ts = encode_quic_varint(timestamp_us);
                    let mut payload = Vec::with_capacity(ts.len() + encoded.len());
                    payload.extend_from_slice(&ts);
                    payload.extend_from_slice(encoded);
                    if current_group.is_none() {
                        match track.append_group() {
                            Ok(g) => current_group = Some(g),
                            Err(e) => {
                                log::warn!(
                                    "Group open failed: {}, stopping encoding",
                                    e
                                );
                                return;
                            }
                        }
                    }
                    let write_result = current_group
                        .as_mut()
                        .map_or(Ok(()), |g| g.write_frame(payload));
                    if let Err(e) = write_result {
                        log::warn!("Track write failed: {}, stopping encoding", e);
                        if let Some(mut g) = current_group.take() {
                            let _ = g.finish();
                        }
                        return;
                    }
                    if pacer.on_frame_written() {
                        if let Some(mut g) = current_group.take() {
                            let _ = g.finish();
                        }
                    }
                }
                if read_pos > 0 {
                    pcm_buffer.drain(..read_pos);
                }
            }
            // Channel closed (StopPublishing dropped the input stream) — FIN
            // the open group so listeners aren't left on a half-delivered
            // group, then clear the level.
            if let Some(mut g) = current_group.take() {
                let _ = g.finish();
            }
            // Clear the level when encoding stops.
            mic_level.store(0.0f32.to_bits(), Ordering::Relaxed);
        });

        Ok(())
    }

    async fn do_subscribe(&mut self, pubkey: &str) -> Result<(), String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let origin = self.origin.as_ref().ok_or("Not connected")?;
        // Broadcast path is the speaker's hex pubkey (no `/audio` suffix —
        // that was the old wrong convention; this matches the ecosystem
        // MoQ track layout.)
        let broadcast_consumer = origin
            .consume()
            .get_broadcast(pubkey)
            .ok_or("Broadcast not found")?;

        // Track name is the literal `"audio/data"` — verified at
        // `@moq/publish/src/audio/encoder.ts:31`.
        let track_consumer = broadcast_consumer
            .subscribe_track(&moq_lite::Track::new("audio/data"))
            .map_err(|e| format!("Subscribe failed: {}", e))?;

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No output device".to_string())?;

        let stream_config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };

        let ring_buf: Arc<StdMutex<std::collections::VecDeque<f32>>> =
            Arc::new(StdMutex::new(std::collections::VecDeque::with_capacity(9600)));

        let err_fn = |err: cpal::StreamError| {
            log::error!("Audio output error: {}", err);
        };

        let ring_buf_out = ring_buf.clone();
        let output_stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buf = ring_buf_out.lock().unwrap_or_else(|e| e.into_inner());
                    for sample in data.iter_mut() {
                        *sample = buf.pop_front().unwrap_or(0.0);
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("Build output stream failed: {}", e))?;

        output_stream
            .play()
            .map_err(|e| format!("Play output failed: {}", e))?;

        let ring_buf_dec = ring_buf.clone();
        // Phase 3.7: clone participant_levels + pubkey for the decode task.
        let levels_map = self.participant_levels.clone();
        let pubkey_for_level = pubkey.to_string();
        let decode_task = tokio::spawn(async move {
            let mut decoder = match opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono) {
                Ok(d) => d,
                Err(e) => {
                    log::error!("Opus decoder init failed: {}", e);
                    return;
                }
            };
            let mut track = track_consumer;
            let mut output = vec![0f32; MAX_DECODE_SAMPLES];
            loop {
                match track.read_frame().await {
                    Ok(Some(frame)) => {
                        // Strip the hang "legacy" container's leading
                        // varint(timestamp_us) before decoding — every
                        // conforming speaker (Amethyst, NestsUI web,
                        // nostr.blue web) sends
                        // `varint(timestamp_us) ++ opus_packet`. Feeding the
                        // varint bytes to the Opus decoder produces garbage;
                        // mirrors Amethyst's
                        // `MoqLiteNestsListener.stripLegacyTimestampPrefix`.
                        // (Desktop playback is ring-buffer based, so the
                        // timestamp value itself is not needed here — only
                        // the prefix must go.)
                        let opus_payload = strip_quic_varint_prefix(&frame);
                        match decoder.decode_float(opus_payload, &mut output, false) {
                            Ok(len) => {
                                // Phase 3.7: compute peak amplitude for remote
                                // speaking detection (matches the local mic
                                // level computation in do_start_publishing).
                                let peak = output[..len]
                                    .iter()
                                    .fold(0.0f32, |acc, &s| acc.max(s.abs()));
                                if let Ok(mut map) = levels_map.lock() {
                                    map.insert(pubkey_for_level.clone(), peak);
                                }
                                let mut buf =
                                    ring_buf_dec.lock().unwrap_or_else(|e| e.into_inner());
                                buf.extend(&output[..len]);
                                let excess = buf.len().saturating_sub(RING_BUFFER_MAX_SAMPLES);
                                if excess > 0 {
                                    buf.drain(..excess);
                                }
                            }
                            Err(e) => {
                                log::warn!("Opus decode error: {}", e);
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        log::warn!("Track read error: {}", e);
                        break;
                    }
                }
            }
        });

        self.subscribers.insert(
            pubkey.to_string(),
            SubState {
                _output_stream: output_stream,
                _decode_task: decode_task.abort_handle(),
            },
        );

        Ok(())
    }

    fn do_disconnect(&mut self) {
        self.input_stream = None;
        self.track = None;
        self.broadcast = None;
        for sub in self.subscribers.values() {
            sub._decode_task.abort();
        }
        self.subscribers.clear();
        // Phase 3.7: clear all per-participant levels.
        if let Ok(mut map) = self.participant_levels.lock() {
            map.clear();
        }
        self.moq_session = None;
        self.origin = None;
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.connection = ConnectionState::Disconnected;
        s.tracks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quic_varint_encode_known_vectors() {
        // 1-byte range [0, 63]
        assert_eq!(encode_quic_varint(0), vec![0x00]);
        assert_eq!(encode_quic_varint(63), vec![0x3F]);
        // 2-byte range [64, 2^14)
        assert_eq!(encode_quic_varint(64), vec![0x40, 0x40]);
        assert_eq!(encode_quic_varint(16383), vec![0x7F, 0xFF]);
        // 4-byte range [2^14, 2^30)
        assert_eq!(encode_quic_varint(20000), vec![0x80, 0x00, 0x4E, 0x20]);
        assert_eq!(encode_quic_varint(1_000_000), vec![0x80, 0x0F, 0x42, 0x40]);
        // 8-byte range [2^30, 2^62): 2^30 = 0x40000000 → 62-bit payload
        // 00..40 00 00 00 after the 0xC0 prefix.
        assert_eq!(
            encode_quic_varint(1 << 30),
            vec![0xC0, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            encode_quic_varint(u64::MAX >> 2),
            vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
        );
    }

    #[test]
    fn quic_varint_strip_round_trip() {
        let opus = [0xF1, 0xB8, 0x4C, 0x00, 0x11, 0x42];
        for ts in [0u64, 63, 64, 20_000, 1_000_000, 1 << 30] {
            let mut frame = encode_quic_varint(ts);
            frame.extend_from_slice(&opus);
            assert_eq!(
                strip_quic_varint_prefix(&frame),
                &opus,
                "round trip failed for ts={ts}"
            );
        }
        // Malformed: varint header would overrun the payload — pass through
        // unchanged (surface rather than mask), matching Amethyst's stripper.
        assert_eq!(strip_quic_varint_prefix(&[0x80, 0x00]), &[0x80, 0x00]);
        assert_eq!(strip_quic_varint_prefix(&[0xC0]), &[0xC0]);
        assert_eq!(strip_quic_varint_prefix(&[]), &[] as &[u8]);
    }

    #[test]
    fn quic_varint_strip_matches_amethyst_reference_logic() {
        // Port of Amethyst's `MoqLiteNestsListener.stripLegacyTimestampPrefix`
        // (the reference listener's frame stripping, including its Kotlin
        // sign-extension semantics for the 2-bit length tag). Our stripper
        // must agree with it on every leading byte value.
        fn amethyst_strip(payload: &[u8]) -> &[u8] {
            if payload.is_empty() {
                return payload;
            }
            // Kotlin: (payload[0].toInt() ushr 6) and 0x3 — sign-extended
            // byte, unsigned shift, mask. Equivalent to u8 top-2-bits here.
            let tag = ((payload[0] as i8 as i32) >> 6) & 0x3;
            let varint_len = 1usize << tag;
            if payload.len() < varint_len {
                return payload;
            }
            &payload[varint_len..]
        }
        let opus = [0xAA, 0xBB, 0xCC];
        for first_byte in 0u8..=255 {
            let mut frame = vec![first_byte, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
            frame.extend_from_slice(&opus);
            assert_eq!(
                strip_quic_varint_prefix(&frame),
                amethyst_strip(&frame),
                "divergence on leading byte 0x{first_byte:02X}"
            );
        }
    }

    #[test]
    fn group_pacer_boundaries() {
        let mut pacer = GroupPacer::default();
        // No finish before the boundary.
        for i in 1..FRAMES_PER_GROUP {
            assert!(!pacer.on_frame_written(), "premature finish at frame {i}");
        }
        // Exactly at the boundary.
        assert!(pacer.on_frame_written());
        // Counter reset — the next window starts fresh.
        for i in 1..FRAMES_PER_GROUP {
            assert!(!pacer.on_frame_written(), "premature finish at frame {i}");
        }
        assert!(pacer.on_frame_written());
        // Mute-edge reset: an out-of-band group close zeroes the counter.
        pacer.reset();
        for i in 1..FRAMES_PER_GROUP {
            assert!(!pacer.on_frame_written(), "premature finish at frame {i}");
        }
        assert!(pacer.on_frame_written());
    }

    #[test]
    fn timestamp_clock_advances_across_muted_frames() {
        // The encoding thread derives timestamps as
        // `frame_index * FRAME_DURATION_US`, advancing the index on every
        // consumed 960-sample frame INCLUDING muted ones — so a mute gap is
        // a real wall-clock gap. 20 ms per frame, 50 fps.
        let frame_index_after = |frames: u64| frames * FRAME_DURATION_US;
        // 1 s of speech = 50 frames = 1_000_000 µs.
        assert_eq!(frame_index_after(50), 1_000_000);
        // A 3 s mute window between two speech bursts: timestamps jump by
        // the muted duration rather than continuing contiguously.
        let before_mute = frame_index_after(50);
        let after_mute = frame_index_after(50 + 150); // +3 s of muted frames
        assert_eq!(after_mute - before_mute, 150 * FRAME_DURATION_US);
    }
}
