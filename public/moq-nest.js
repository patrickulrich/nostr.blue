window.nestAudioManager = window.nestAudioManager || {
    connections: new Map(),
    _moqModule: null,
    _moqLoading: null,

    async _loadMoq() {
        if (this._moqModule) return this._moqModule;
        if (this._moqLoading) return this._moqLoading;
        this._moqLoading = (async () => {
            try {
                this._moqModule = await import('https://esm.sh/@moq/net@0.1.2');
                return this._moqModule;
            } catch (e) {
                this._moqLoading = null;
                throw e;
            }
        })();
        return this._moqLoading;
    },

    _getConnection(publisherId) {
        return this.connections.get(publisherId);
    },

    async init(publisherId) {
        if (this.connections.has(publisherId)) return { type: 'success' };
        this.connections.set(publisherId, {
            state: 'disconnected',
            connection: null,
            myPubkeyHex: null,
            publisherBroadcast: null,
            publisherTracks: new Set(),
            subscribers: new Map(),
            audioEncoder: null,
            mediaStream: null,
            micMuted: false,
            micLevel: 0.0,
            error: null,
            participantTracks: [],
            announcedParticipants: [],
            announceTask: null,
        });
        try {
            await this._loadMoq();
            return { type: 'success' };
        } catch (e) {
            const conn = this._getConnection(publisherId);
            if (conn) {
                conn.state = 'error';
                conn.error = 'Failed to load MoQ library: ' + (e.message || e);
            }
            return { type: 'error', error: 'Failed to load MoQ library: ' + (e.message || e) };
        }
    },

    async connect(publisherId, authUrl, relayUrl, namespace, jwt, myPubkeyHex) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'error', error: 'Not initialized' };
        try {
            conn.state = 'connecting';
            const Moq = this._moqModule;
            const url = new URL(relayUrl);
            url.pathname = '/' + namespace;
            // NOTE: WebTransport does not support custom headers on the HTTP/3 CONNECT
            // request, so the JWT must be passed as a URL query parameter. This is a
            // known security trade-off: the token may appear in server access logs,
            // browser history, and Referer headers. Mitigation: ensure the JWT has a
            // short TTL (e.g., 60 seconds).
            if (jwt) url.searchParams.set('jwt', jwt);

            // Phase 4.2: WebSocket fallback for browsers without WebTransport
            // (~19% of browsers, including Android WebView on older versions).
            // Verified API at `@moq/net/src/connection/connect.ts:12-35` — the
            // `websocket` option enables a QMux-over-WebSocket fallback that
            // races WebTransport (500ms head start) vs WebSocket.
            conn.connection = await Moq.Connection.connect(url, { websocket: {} });
            conn.myPubkeyHex = myPubkeyHex;
            conn.state = 'connected';

            // Phase 4.1: Start ANNOUNCE subscription for real-time participant
            // discovery. The announced() method returns an async iterator of
            // AnnouncedEntry { path, active }. We filter to 64-char hex pubkeys
            // (excluding catalog tracks, chat, etc.) and skip our own pubkey.
            // Matches `NestsUI-v2/src/transport/moq-transport.ts:362-444`.
            this._startAnnounceLoop(publisherId);

            return { type: 'success' };
        } catch (e) {
            conn.state = 'error';
            conn.error = e.message || String(e);
            return { type: 'error', error: conn.error };
        }
    },

    /// Phase 4.1: Subscribe to the MoQ ANNOUNCE stream and cache the list of
    /// announced participant pubkeys. Polled by Rust every 3s.
    _startAnnounceLoop(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn || !conn.connection) return;
        if (conn.announceTask) return; // already running

        const pubkeyRegex = /^[0-9a-f]{64}$/;
        conn.announceTask = (async () => {
            try {
                const announced = conn.connection.announced();
                for (;;) {
                    const entry = await announced.next();
                    if (!entry) break;
                    // Filter to 64-char hex pubkeys, skip self.
                    if (!pubkeyRegex.test(entry.path)) continue;
                    if (entry.path === conn.myPubkeyHex) continue;
                    if (entry.active) {
                        if (!conn.announcedParticipants.includes(entry.path)) {
                            conn.announcedParticipants.push(entry.path);
                        }
                    } else {
                        conn.announcedParticipants = conn.announcedParticipants.filter(
                            (p) => p !== entry.path,
                        );
                    }
                }
            } catch (e) {
                // Connection closed or error — non-fatal, the loop just ends.
                if (conn.state === 'connected' || conn.state === 'publishing') {
                    console.warn('[MoQ Nest] Announce loop ended:', e);
                }
            }
        })();
    },

    /// Phase 4.1: Return the list of announced participant pubkeys.
    /// Called by Rust every 3s via `pollAnnouncedParticipants`.
    pollAnnouncedParticipants(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return [];
        return conn.announcedParticipants || [];
    },

    async publishAudio(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'error', error: 'Not initialized' };
        if (!conn.connection) return { type: 'error', error: 'Not connected' };
        if (conn.publisherBroadcast) return { type: 'success' };
        try {
            const Moq = this._moqModule;
            conn.state = 'publishing';

            const broadcast = new Moq.Broadcast();
            conn.publisherBroadcast = broadcast;
            conn.connection.publish(Moq.Path.from(conn.myPubkeyHex), broadcast);

            (async () => {
                try {
                    for (;;) {
                        const request = await broadcast.requested();
                        if (!request) break;
                        if (request.track.name === 'audio/data') {
                            conn.publisherTracks.add(request.track);
                            request.track.closed.then(() => {
                                conn.publisherTracks.delete(request.track);
                            });
                        }
                    }
                } catch (e) {
                    if (conn.state === 'publishing') {
                        console.error('[MoQ Nest] Publisher requested() error:', e);
                    }
                }
            })();

            conn.mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
            const processor = new MediaStreamTrackProcessor({ track: conn.mediaStream.getAudioTracks()[0] });
            const reader = processor.readable.getReader();

            conn.audioEncoder = new AudioEncoder({
                output: (chunk, _metadata) => {
                    const buffer = new ArrayBuffer(chunk.byteLength);
                    chunk.copyTo(buffer);
                    const data = new Uint8Array(buffer);
                    const varintBytes = Moq.Varint.encode(chunk.timestamp);
                    const frame = new Uint8Array(varintBytes.length + data.length);
                    frame.set(varintBytes, 0);
                    frame.set(data, varintBytes.length);
                    for (const track of conn.publisherTracks) {
                        try { track.writeFrame(frame); } catch (_) {}
                    }
                },
                error: (e) => {
                    console.error('[MoQ Nest] AudioEncoder error:', e);
                },
            });
            conn.audioEncoder.configure({
                codec: 'opus',
                sampleRate: 48000,
                numberOfChannels: 1,
                bitrate: 64000,
            });

            // Phase 1.5: Reusable buffer for peak amplitude computation.
            // We compute the mic level from each raw PCM frame before encoding,
            // matching Amethyst's `peakAmplitude` approach.
            let peakBuffer = new Float32Array(480); // small reuse buffer

            (async () => {
                try {
                    while (true) {
                        const result = await reader.read();
                        if (result.done) break;
                        if (!conn.micMuted && conn.audioEncoder.state === 'configured') {
                            // Phase 1.5: compute peak amplitude for speaking detection.
                            try {
                                const numFrames = result.value.numberOfFrames;
                                if (numFrames > peakBuffer.length) {
                                    peakBuffer = new Float32Array(numFrames);
                                }
                                result.value.copyTo(peakBuffer, { planeIndex: 0 });
                                let peak = 0.0;
                                for (let i = 0; i < numFrames; i++) {
                                    const abs = Math.abs(peakBuffer[i]);
                                    if (abs > peak) peak = abs;
                                }
                                conn.micLevel = peak;
                            } catch (_) {
                                // Non-fatal — skip level update if copyTo fails.
                            }
                            conn.audioEncoder.encode(result.value);
                        }
                        result.value.close();
                    }
                } catch (e) {
                    if (conn.state === 'publishing') {
                        console.error('[MoQ Nest] Audio read loop error:', e);
                    }
                }
            })();
            return { type: 'success' };
        } catch (e) {
            conn.state = 'error';
            conn.error = e.message || String(e);
            return { type: 'error', error: conn.error };
        }
    },

    /// Phase 1.5: Return the current mic peak level (0.0–1.0).
    /// Polled by Rust every 100ms for energy-gated speaking detection.
    getMicLevel(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return 0.0;
        return conn.micLevel || 0.0;
    },

    /// Phase 3.7: Return all subscribed participant peak levels as a
    /// `{pubkey: level}` map. Polled as a batch every 100ms by Rust for
    /// remote speaking detection — one JS eval per tick regardless of
    /// participant count.
    getAllParticipantLevels(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return {};
        const result = {};
        for (const [pk, sub] of conn.subscribers) {
            result[pk] = sub.level || 0.0;
        }
        return result;
    },

    async subscribeAudio(publisherId, participantPubkey) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'error', error: 'Not initialized' };
        if (!conn.connection) return { type: 'error', error: 'Not connected' };
        if (conn.subscribers.has(participantPubkey)) return { type: 'success' };
        try {
            const Moq = this._moqModule;
            const broadcast = conn.connection.consume(Moq.Path.from(participantPubkey));
            const track = broadcast.subscribe('audio/data', 128);

            const audioContext = new AudioContext({ sampleRate: 48000 });
            // Phase 3.2: Per-speaker GainNode for individual volume control.
            // Inserted between each decoded source and the destination.
            const gainNode = audioContext.createGain();
            gainNode.gain.value = 1.0;
            gainNode.connect(audioContext.destination);

            const subState = {
                broadcast,
                track,
                audioContext,
                gainNode,
                audioDecoder: null,
                active: true,
                // Phase 2.2: last frame timestamp (Unix ms) for cliff detection.
                lastFrameMs: 0,
                // Phase 3.7: peak audio level (0.0–1.0) for remote speaking
                // detection. Updated per decoded frame in the AudioDecoder
                // output callback.
                level: 0.0,
            };
            const bufferPool = [];
            const POOL_SIZE = 10;
            subState.audioDecoder = new AudioDecoder({
                output: (audioData) => {
                    try {
                        const numFrames = audioData.numberOfFrames;
                        const numChannels = audioData.numberOfChannels;
                        const sampleRate = audioData.sampleRate;
                        const buffer = new Float32Array(numFrames * numChannels);
                        audioData.copyTo(buffer, { planeIndex: 0 });
                        // Phase 3.7: compute peak amplitude for remote speaking
                        // detection. Matches Amethyst's `peakAmplitude` and
                        // the local mic level computation in publishAudio.
                        let peak = 0.0;
                        for (let i = 0; i < buffer.length; i++) {
                            const abs = Math.abs(buffer[i]);
                            if (abs > peak) peak = abs;
                        }
                        subState.level = peak;
                        let audioBuffer;
                        while (bufferPool.length > 0) {
                            const cached = bufferPool.pop();
                            if (cached.numberOfFrames === numFrames && cached.numberOfChannels === numChannels) {
                                audioBuffer = cached;
                                break;
                            }
                        }
                        if (!audioBuffer) {
                            audioBuffer = subState.audioContext.createBuffer(numChannels, numFrames, sampleRate);
                        }
                        audioBuffer.copyToChannel(buffer, 0);
                        const source = subState.audioContext.createBufferSource();
                        source.buffer = audioBuffer;
                        // Phase 3.2: route through the per-speaker GainNode.
                        source.connect(subState.gainNode);
                        source.onended = () => {
                            if (bufferPool.length < POOL_SIZE) bufferPool.push(audioBuffer);
                        };
                        source.start();
                    } catch (e) { console.warn('[MoQ Nest] Audio playback error:', e); }
                    audioData.close();
                },
                error: (e) => {
                    console.error('[MoQ Nest] AudioDecoder error:', e);
                },
            });
            subState.audioDecoder.configure({
                codec: 'opus',
                sampleRate: 48000,
                numberOfChannels: 1,
            });
            (async () => {
                try {
                    while (subState.active) {
                        const frame = await track.readFrame();
                        if (!subState.active || !frame) break;
                        // Phase 2.2: record frame arrival for cliff detection.
                        subState.lastFrameMs = Date.now();
                        const [timestampUs, payload] = Moq.Varint.decode(frame);
                        if (subState.audioDecoder.state === 'configured') {
                            const data = new EncodedAudioChunk({
                                type: 'key',
                                timestamp: timestampUs,
                                data: payload,
                            });
                            subState.audioDecoder.decode(data);
                        }
                    }
                } catch (e) {
                    if (subState.active) {
                        console.error('[MoQ Nest] Subscription read error:', e);
                    }
                }
            })();
            conn.subscribers.set(participantPubkey, subState);
            if (!conn.participantTracks.includes(participantPubkey)) {
                conn.participantTracks.push(participantPubkey);
            }
            return { type: 'success' };
        } catch (e) {
            return { type: 'error', error: e.message || String(e) };
        }
    },

    /// Phase 2.2: Return the last frame arrival time (Unix ms) for a
    /// participant. Used by the cliff detector in nest_viewer.rs.
    getLastFrameMs(publisherId, participantPubkey) {
        const conn = this._getConnection(publisherId);
        if (!conn) return 0;
        const sub = conn.subscribers.get(participantPubkey);
        return sub ? sub.lastFrameMs : 0;
    },

    /// Phase 3.2: Set per-speaker volume (0.0–1.0). Applies a GainNode value
    /// to one speaker without affecting the rest ("local hush").
    setVolume(publisherId, participantPubkey, volume) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'error', error: 'Not initialized' };
        const sub = conn.subscribers.get(participantPubkey);
        if (sub && sub.gainNode) {
            sub.gainNode.gain.value = Math.max(0.0, Math.min(1.0, volume));
            return { type: 'success' };
        }
        return { type: 'error', error: 'Participant not subscribed' };
    },

    async unsubscribeAudio(publisherId, participantPubkey) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'success' };
        const subState = conn.subscribers.get(participantPubkey);
        if (subState) {
            subState.active = false;
            try { if (subState.track) subState.track.close(); } catch (_) {}
            try { if (subState.audioDecoder) subState.audioDecoder.close(); } catch (_) {}
            try { if (subState.audioContext) subState.audioContext.close(); } catch (_) {}
            conn.subscribers.delete(participantPubkey);
            conn.participantTracks = conn.participantTracks.filter(t => t !== participantPubkey);
        }
        return { type: 'success' };
    },

    async mute(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'error', error: 'Not initialized' };
        conn.micMuted = true;
        if (conn.audioEncoder && conn.audioEncoder.state === 'configured') {
            try { conn.audioEncoder.reset(); } catch (_) {}
        }
        return { type: 'success' };
    },

    async unmute(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'error', error: 'Not initialized' };
        conn.micMuted = false;
        return { type: 'success' };
    },

    async disconnect(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'success' };
        try {
            for (const [_pubkey, sub] of conn.subscribers.entries()) {
                sub.active = false;
                try { if (sub.track) sub.track.close(); } catch (_) {}
                try { if (sub.audioDecoder) sub.audioDecoder.close(); } catch (_) {}
                try { if (sub.audioContext) sub.audioContext.close(); } catch (_) {}
            }
            conn.subscribers.clear();
            if (conn.audioEncoder) {
                try { conn.audioEncoder.close(); } catch (_) {}
                conn.audioEncoder = null;
            }
            if (conn.mediaStream) {
                conn.mediaStream.getTracks().forEach(t => t.stop());
                conn.mediaStream = null;
            }
            if (conn.publisherBroadcast) {
                try { conn.publisherBroadcast.close(); } catch (_) {}
                conn.publisherBroadcast = null;
            }
            conn.publisherTracks.clear();
            if (conn.connection) {
                try { conn.connection.close(); } catch (_) {}
                conn.connection = null;
            }
            conn.announceTask = null;
            conn.announcedParticipants = [];
            conn.state = 'disconnected';
            conn.error = null;
            conn.participantTracks = [];
            conn.micLevel = 0.0;
        } catch (e) {
            console.error('[MoQ Nest] Disconnect error:', e);
        }
        this.connections.delete(publisherId);
        return { type: 'success' };
    },

    getConnectionState(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return 'disconnected';
        return conn.state;
    },

    getParticipantTracks(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return [];
        return conn.participantTracks;
    },
};
