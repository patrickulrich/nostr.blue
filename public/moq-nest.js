window.nestAudioManager = window.nestAudioManager || {
    connections: new Map(),
    _moqModule: null,
    _moqLoading: null,

    async _loadMoq() {
        if (this._moqModule) return this._moqModule;
        if (this._moqLoading) return this._moqLoading;
        this._moqLoading = (async () => {
            try {
                this._moqModule = await import('https://esm.sh/@kixelated/moq@0.9.4');
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
            session: null,
            publisher: null,
            subscribers: new Map(),
            audioEncoder: null,
            audioDecoder: null,
            audioContext: null,
            mediaStream: null,
            micMuted: false,
            namespace: null,
            error: null,
            participantTracks: [],
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

    async connect(publisherId, authUrl, relayUrl, namespace, jwt) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'error', error: 'Not initialized' };
        try {
            conn.state = 'connecting';
            const url = new URL(relayUrl);
            if (jwt) url.searchParams.set('jwt', jwt);
            conn.session = new this._moqModule.Session(url.toString());
            conn.namespace = namespace;
            conn.state = 'connected';
            return { type: 'success' };
        } catch (e) {
            conn.state = 'error';
            conn.error = e.message || String(e);
            return { type: 'error', error: conn.error };
        }
    },

    async publishAudio(publisherId) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'error', error: 'Not initialized' };
        if (!conn.session) return { type: 'error', error: 'Not connected' };
        try {
            conn.state = 'publishing';
            conn.mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
            conn.audioContext = new AudioContext({ sampleRate: 48000 });
            const source = conn.audioContext.createMediaStreamSource(conn.mediaStream);
            const processor = new MediaStreamTrackProcessor({ track: conn.mediaStream.getAudioTracks()[0] });
            const reader = processor.readable.getReader();
            conn.publisher = conn.session.publish(conn.namespace);
            const track = conn.publisher.createTrack('audio');
            conn.audioEncoder = new AudioEncoder({
                output: (chunk, _metadata) => {
                    const buffer = new ArrayBuffer(chunk.byteLength);
                    chunk.copyTo(buffer);
                    track.writeChunk(buffer);
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
            (async () => {
                try {
                    while (true) {
                        const result = await reader.read();
                        if (result.done) break;
                        if (conn.micMuted) continue;
                        if (conn.audioEncoder.state === 'configured') {
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

    async subscribeAudio(publisherId, participantPubkey) {
        const conn = this._getConnection(publisherId);
        if (!conn) return { type: 'error', error: 'Not initialized' };
        if (!conn.session) return { type: 'error', error: 'Not connected' };
        if (conn.subscribers.has(participantPubkey)) return { type: 'success' };
        try {
            const trackName = participantPubkey + '/audio';
            const subscription = conn.session.subscribe(conn.namespace, trackName);
            const subState = {
                subscription,
                audioContext: new AudioContext({ sampleRate: 48000 }),
                audioDecoder: null,
                active: true,
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
                        const audioBuffer = bufferPool.length > 0
                            ? bufferPool.pop()
                            : subState.audioContext.createBuffer(numChannels, numFrames, sampleRate);
                        audioBuffer.copyToChannel(buffer, 0);
                        const source = subState.audioContext.createBufferSource();
                        source.buffer = audioBuffer;
                        source.connect(subState.audioContext.destination);
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
            let frameIndex = 0;
            (async () => {
                try {
                    while (subState.active) {
                        const chunk = await subscription.readChunk();
                        if (!subState.active) break;
                        if (subState.audioDecoder.state === 'configured') {
                            const data = new EncodedAudioChunk({
                                type: 'key',
                                timestamp: frameIndex++ * 20000,
                                data: chunk,
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
                try { if (sub.audioDecoder) sub.audioDecoder.close(); } catch (_) {}
                try { if (sub.audioContext) sub.audioContext.close(); } catch (_) {}
            }
            conn.subscribers.clear();
            if (conn.audioEncoder) {
                try { conn.audioEncoder.close(); } catch (_) {}
                conn.audioEncoder = null;
            }
            if (conn.audioContext) {
                try { conn.audioContext.close(); } catch (_) {}
                conn.audioContext = null;
            }
            if (conn.mediaStream) {
                conn.mediaStream.getTracks().forEach(t => t.stop());
                conn.mediaStream = null;
            }
            conn.publisher = null;
            if (conn.session) {
                try { conn.session.close(); } catch (_) {}
                conn.session = null;
            }
            conn.state = 'disconnected';
            conn.error = null;
            conn.participantTracks = [];
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
