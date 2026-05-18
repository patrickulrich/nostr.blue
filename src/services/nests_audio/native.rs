use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::{mpsc, oneshot};

use super::ConnectionState;

const SAMPLE_RATE: u32 = 48000;
const OPUS_FRAME_SIZE: usize = 960;

enum NativeCmd {
    Connect {
        relay_url: String,
        namespace: String,
        jwt: String,
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
}

#[derive(Clone)]
pub struct NativeBridge {
    cmd_tx: mpsc::UnboundedSender<NativeCmd>,
    shared: Arc<StdMutex<SharedState>>,
}

impl NativeBridge {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let shared = Arc::new(StdMutex::new(SharedState {
            connection: ConnectionState::Disconnected,
            tracks: Vec::new(),
        }));
        let shared_clone = shared.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create audio runtime");
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
    ) -> Result<(), String> {
        self.set_state(ConnectionState::Connecting);
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NativeCmd::Connect {
                relay_url: relay_url.to_string(),
                namespace: namespace.to_string(),
                jwt: jwt.to_string(),
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
        let mut s = self.shared.lock().unwrap();
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
        let mut s = self.shared.lock().unwrap();
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
        self.shared.lock().unwrap().connection.clone()
    }

    pub fn participant_tracks(&self) -> Vec<String> {
        self.shared.lock().unwrap().tracks.clone()
    }

    fn set_state(&self, state: ConnectionState) {
        self.shared.lock().unwrap().connection = state;
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
                reply,
            } => {
                let _ = reply.send(self.do_connect(&relay_url, &namespace, &jwt).await);
            }
            NativeCmd::StartPublishing { reply } => {
                let _ = reply.send(self.do_start_publishing());
            }
            NativeCmd::StopPublishing { reply } => {
                self.input_stream = None;
                let _ = reply.send(Ok(()));
            }
            NativeCmd::SetMuted { muted: _, reply } => {
                let _ = reply.send(Ok(()));
            }
            NativeCmd::SubscribeToParticipant { pubkey, reply } => {
                let _ = reply.send(self.do_subscribe(&pubkey).await);
            }
            NativeCmd::UnsubscribeFromParticipant { pubkey, reply } => {
                if let Some(sub) = self.subscribers.remove(&pubkey) {
                    sub._decode_task.abort();
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
    ) -> Result<(), String> {
        let mut url = url::Url::parse(relay_url).map_err(|e| format!("Invalid URL: {}", e))?;

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
        let track = broadcast_producer
            .create_track(moq_lite::Track::new("audio"))
            .map_err(|e| format!("Create track failed: {}", e))?;

        origin_producer.publish_broadcast(namespace, broadcast_producer.consume());

        self.moq_session = Some(session);
        self.origin = Some(origin_producer);
        self.broadcast = Some(broadcast_producer);
        self.track = Some(track);

        Ok(())
    }

    fn do_start_publishing(&mut self) -> Result<(), String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

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
            while let Ok(samples) = audio_rx.recv() {
                pcm_buffer.extend_from_slice(&samples);
                while pcm_buffer.len() >= OPUS_FRAME_SIZE {
                    let frame: Vec<f32> = pcm_buffer.drain(..OPUS_FRAME_SIZE).collect();
                    match encoder.encode_float(&frame, &mut output) {
                        Ok(len) => {
                            let encoded = output[..len].to_vec();
                            let mut t = track.clone();
                            if let Err(e) = t.write_frame(encoded) {
                                log::warn!("Track write failed: {}, stopping encoding", e);
                                return;
                            }
                        }
                        Err(e) => {
                            log::warn!("Opus encode error: {}", e);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn do_subscribe(&mut self, pubkey: &str) -> Result<(), String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let origin = self.origin.as_ref().ok_or("Not connected")?;
        let path = format!("{}/audio", pubkey);
        let broadcast_consumer = origin
            .consume()
            .get_broadcast(path.as_str())
            .ok_or("Broadcast not found")?;

        let track_consumer = broadcast_consumer
            .subscribe_track(&moq_lite::Track::new("audio"))
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

        let (pcm_tx, pcm_rx) = std::sync::mpsc::channel::<Vec<f32>>();

        let err_fn = |err: cpal::StreamError| {
            log::error!("Audio output error: {}", err);
        };

        let output_stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if let Ok(samples) = pcm_rx.try_recv() {
                        let len = data.len().min(samples.len());
                        data[..len].copy_from_slice(&samples[..len]);
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("Build output stream failed: {}", e))?;

        output_stream
            .play()
            .map_err(|e| format!("Play output failed: {}", e))?;

        let decode_task = tokio::spawn(async move {
            let mut decoder = match opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono) {
                Ok(d) => d,
                Err(e) => {
                    log::error!("Opus decoder init failed: {}", e);
                    return;
                }
            };
            let mut track = track_consumer;
            let mut output = vec![0f32; OPUS_FRAME_SIZE * 2];
            loop {
                match track.read_frame().await {
                    Ok(Some(frame)) => match decoder.decode_float(&frame, &mut output, false) {
                        Ok(len) => {
                            let samples = output[..len].to_vec();
                            let _ = pcm_tx.send(samples);
                        }
                        Err(e) => {
                            log::warn!("Opus decode error: {}", e);
                        }
                    },
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
        self.subscribers.clear();
        self.moq_session = None;
        self.origin = None;
        let mut s = self.shared.lock().unwrap();
        s.connection = ConnectionState::Disconnected;
        s.tracks.clear();
    }
}
