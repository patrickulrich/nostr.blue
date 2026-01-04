/**
 * HLS Manager for Radio Streams
 * Lazy-loads hls.js and provides a unified interface for both HLS and native audio playback
 */
window.hlsManager = window.hlsManager || {
    instances: new Map(),
    hlsLoaded: false,
    hlsLoading: null,
    nowPlaying: null, // Current HLS metadata {title, artist}
    nowPlayingAudioId: null, // Track which audio element owns nowPlaying

    /**
     * Lazy load hls.js from CDN
     */
    async loadHls() {
        if (window.Hls) {
            this.hlsLoaded = true;
            return;
        }

        // Return existing promise if already loading
        if (this.hlsLoading) {
            return this.hlsLoading;
        }

        this.hlsLoading = new Promise((resolve, reject) => {
            const script = document.createElement('script');
            script.src = 'https://cdn.jsdelivr.net/npm/hls.js@1.5.7/dist/hls.min.js';
            script.crossOrigin = 'anonymous';
            script.integrity = 'sha384-1B+J55elPxu+trIhW7QThjZg3evX8C5P6zjB82Xnn46RKPAXpL+vkanRSjCidsJv';
            script.onload = () => {
                console.log('[HLS Manager] hls.js loaded');
                this.hlsLoaded = true;
                resolve();
            };
            script.onerror = () => {
                this.hlsLoading = null;
                reject(new Error('Failed to load hls.js'));
            };
            document.head.appendChild(script);
        });

        return this.hlsLoading;
    },

    /**
     * Check if URL is an HLS stream
     */
    isHlsUrl(url) {
        if (!url) return false;
        try {
            const parsed = new URL(url, window.location.origin);
            // Case-insensitive check to match fallback behavior
            return parsed.pathname.toLowerCase().endsWith('.m3u8');
        } catch {
            // Fallback for invalid URLs: check if path portion ends with .m3u8
            return /\.m3u8(\?|#|$)/i.test(url);
        }
    },

    /**
     * Attach stream to audio element
     * Handles both HLS and native playback automatically
     */
    async attachToAudio(audioId, streamUrl) {
        const audio = document.getElementById(audioId);
        if (!audio) {
            throw new Error('Audio element not found: ' + audioId);
        }

        // Cleanup existing instance
        this.detach(audioId);

        const isHls = this.isHlsUrl(streamUrl);

        if (!isHls) {
            // Native playback for non-HLS streams (MP3, AAC, OGG)
            console.log('[HLS Manager] Using native playback for:', streamUrl);
            audio.src = streamUrl;
            return { type: 'native', url: streamUrl };
        }

        // Check native HLS support (Safari, iOS)
        if (audio.canPlayType('application/vnd.apple.mpegurl')) {
            console.log('[HLS Manager] Using native HLS support');
            audio.src = streamUrl;
            return { type: 'native-hls', url: streamUrl };
        }

        // Use hls.js for browsers without native HLS support
        await this.loadHls();

        if (!Hls.isSupported()) {
            throw new Error('HLS not supported in this browser');
        }

        const hls = new Hls({
            enableWorker: true,
            lowLatencyMode: true,
            backBufferLength: 30,
            maxBufferLength: 60,
            maxMaxBufferLength: 120,
            // Radio stream optimizations
            liveSyncDurationCount: 3,
            liveMaxLatencyDurationCount: 10,
        });

        return new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
                hls.destroy();
                reject(new Error('HLS stream timeout - stream may be offline'));
            }, 15000);

            hls.attachMedia(audio);

            hls.on(Hls.Events.MEDIA_ATTACHED, () => {
                console.log('[HLS Manager] Media attached, loading source:', streamUrl);
                hls.loadSource(streamUrl);
            });

            hls.on(Hls.Events.MANIFEST_PARSED, (event, data) => {
                clearTimeout(timeout);
                console.log('[HLS Manager] Manifest parsed, levels:', data.levels.length);
                this.instances.set(audioId, hls);
                resolve({ type: 'hls.js', levels: data.levels.length, url: streamUrl });
            });

            hls.on(Hls.Events.ERROR, (event, data) => {
                if (data.fatal) {
                    clearTimeout(timeout);
                    console.error('[HLS Manager] Fatal error:', data.type, data.details);
                    hls.destroy();
                    this.instances.delete(audioId);

                    // Dispatch error event for Rust to catch
                    window.dispatchEvent(new CustomEvent('hls-stream-error', {
                        detail: {
                            audioId: audioId,
                            error: data.details,
                            type: data.type
                        }
                    }));

                    reject(new Error('HLS error: ' + data.details));
                } else {
                    // Non-fatal error - hls.js will try to recover
                    console.warn('[HLS Manager] Non-fatal error:', data.details);
                }
            });

            // Listen for ID3 timed metadata (now-playing info in some HLS streams)
            hls.on(Hls.Events.FRAG_PARSING_METADATA, (event, data) => {
                const samples = data.samples || [];
                samples.forEach(sample => {
                    try {
                        const frames = this.parseId3(sample.data);
                        if (frames && (frames.TIT2 || frames.TPE1)) {
                            // Store in hlsManager for polling from Rust
                            this.nowPlayingAudioId = audioId;
                            this.nowPlaying = {
                                title: frames.TIT2 || null,
                                artist: frames.TPE1 || null
                            };
                            console.log('[HLS Manager] Now playing:', this.nowPlaying);

                            // Also dispatch event for any JS listeners
                            window.dispatchEvent(new CustomEvent('hls-metadata', {
                                detail: {
                                    audioId: audioId,
                                    title: frames.TIT2,
                                    artist: frames.TPE1
                                }
                            }));
                        }
                    } catch (e) {
                        // ID3 parsing errors are non-fatal
                        console.debug('[HLS Manager] ID3 parsing error:', e);
                    }
                });
            });
        });
    },

    /**
     * Detach and cleanup HLS instance
     */
    detach(audioId) {
        const hls = this.instances.get(audioId);
        if (hls) {
            console.log('[HLS Manager] Destroying HLS instance for:', audioId);
            hls.destroy();
            this.instances.delete(audioId);
        }
        // Clear now playing metadata only if this audio owned it
        if (this.nowPlayingAudioId === audioId) {
            this.nowPlaying = null;
            this.nowPlayingAudioId = null;
        }
    },

    /**
     * Get available quality levels for HLS stream
     */
    getQualityLevels(audioId) {
        const hls = this.instances.get(audioId);
        if (!hls) return [];

        return hls.levels.map((level, i) => ({
            index: i,
            bitrate: level.bitrate,
            name: level.name || `${Math.round(level.bitrate / 1000)}kbps`
        }));
    },

    /**
     * Set quality level for HLS stream (-1 for auto)
     */
    setQualityLevel(audioId, levelIndex) {
        const hls = this.instances.get(audioId);
        if (hls) {
            hls.currentLevel = levelIndex;
        }
    },

    /**
     * Basic ID3v2 frame parsing for TIT2 (title) and TPE1 (artist)
     */
    parseId3(data) {
        if (!data || data.length < 10) return null;

        const frames = {};

        // Check for ID3 header
        const id3 = String.fromCharCode(data[0], data[1], data[2]);
        if (id3 !== 'ID3') return null;

        // Get ID3 version (byte 3 is major version: 3 for ID3v2.3, 4 for ID3v2.4)
        const majorVersion = data[3];

        let offset = 10; // Skip ID3 header

        while (offset < data.length - 10) {
            const frameId = String.fromCharCode(data[offset], data[offset + 1], data[offset + 2], data[offset + 3]);
            if (frameId === '\0\0\0\0') break;

            // ID3v2.4 uses syncsafe integers (7 bits per byte), v2.3 uses regular integers
            let size;
            if (majorVersion >= 4) {
                // Syncsafe: 7 bits per byte
                size = ((data[offset + 4] & 0x7f) << 21) |
                       ((data[offset + 5] & 0x7f) << 14) |
                       ((data[offset + 6] & 0x7f) << 7) |
                       (data[offset + 7] & 0x7f);
            } else {
                // Use unsigned byte masking to prevent sign extension for values >= 128
                size = ((data[offset + 4] & 0xff) << 24) |
                       ((data[offset + 5] & 0xff) << 16) |
                       ((data[offset + 6] & 0xff) << 8) |
                       (data[offset + 7] & 0xff);
            }
            // Ensure size is positive and fits within remaining buffer (accounting for 10-byte frame header)
            if (size <= 0 || size > data.length - offset - 10) break;

            offset += 10; // Skip frame header

            if (frameId === 'TIT2' || frameId === 'TPE1') {
                // Text encoding byte + text content
                const encoding = data[offset];
                const textData = data.slice(offset + 1, offset + size);

                let text = '';
                if (encoding === 0 || encoding === 3) {
                    // encoding 0 = ISO-8859-1, encoding 3 = UTF-8
                    const charset = encoding === 0 ? 'iso-8859-1' : 'utf-8';
                    text = new TextDecoder(charset).decode(textData);
                } else if (encoding === 1) {
                    // UTF-16 with BOM
                    text = new TextDecoder('utf-16').decode(textData);
                } else if (encoding === 2) {
                    // UTF-16BE without BOM
                    text = new TextDecoder('utf-16be').decode(textData);
                }

                if (text) {
                    frames[frameId] = text.replace(/\0/g, '').trim();
                }
            }

            offset += size;
        }

        return frames;
    }
};
