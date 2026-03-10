/**
 * HLS Manager for Radio Streams
 * Lazy-loads hls.js and provides a unified interface for both HLS and native audio playback
 */
window.hlsManager = window.hlsManager || {
    instances: new Map(),
    pendingTimeouts: new Map(),
    pendingResolves: new Map(),
    hlsLoaded: false,
    hlsLoading: null,
    nowPlaying: null, // Current HLS metadata {title, artist}
    nowPlayingElementId: null, // Track which media element owns nowPlaying
    activeAttachMap: new Map(), // Track active attach operations per elementId

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

        this.hlsLoading = new Promise((resolve) => {
            const script = document.createElement('script');
            script.src = 'https://cdn.jsdelivr.net/npm/hls.js@1.6.13/dist/hls.min.js';
            script.crossOrigin = 'anonymous';
            script.integrity = 'sha384-z+tuLqMWl1/cPv7O+39RO0EURSNvorimpcCaMgeNwU+qFBx+AlUIl7jaAwg0cYil';
            
            let timeoutId = null;
            
            const cleanup = () => {
                if (timeoutId) {
                    clearTimeout(timeoutId);
                    timeoutId = null;
                }
                script.onload = null;
                script.onerror = null;
            };
            
            script.onload = () => {
                cleanup();
                console.log('[HLS Manager] hls.js loaded');
                this.hlsLoaded = true;
                resolve({ type: 'success' });
            };
            
            script.onerror = () => {
                cleanup();
                this.hlsLoading = null;
                this.hlsLoaded = false;
                resolve({ type: 'error', error: 'Failed to load hls.js' });
            };
            
            // Add timeout to prevent hanging on CDN stall
            timeoutId = setTimeout(() => {
                cleanup();
                script.remove();
                this.hlsLoading = null;
                this.hlsLoaded = false;
                resolve({ type: 'error', error: 'Timeout loading hls.js from CDN' });
            }, 10000);
            
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

    clearPendingAttach(elementId, clearResolve = true) {
        const pendingTimeout = this.pendingTimeouts.get(elementId);
        if (pendingTimeout) {
            clearTimeout(pendingTimeout);
            this.pendingTimeouts.delete(elementId);
        }
        if (clearResolve) {
            this.pendingResolves.delete(elementId);
        }
    },

    settlePendingAttach(elementId, result, attachId) {
        const pending = this.pendingResolves.get(elementId);
        if (pending && pending.attachId === attachId) {
            this.pendingResolves.delete(elementId);
            pending.resolve(result);
        }
    },

    /**
     * Attach stream to audio or video element
     * Handles both HLS and native playback automatically
     */
    async attachToMedia(elementId, streamUrl) {
        const element = document.getElementById(elementId);
        if (!element || !(element instanceof HTMLMediaElement)) {
            return {
                type: 'error',
                url: streamUrl,
                error: 'Media element not found or not a HTMLMediaElement: ' + elementId
            };
        }

        const isHls = this.isHlsUrl(streamUrl);

        if (!isHls) {
            // Native playback for non-HLS streams (MP3, AAC, OGG)
            console.log('[HLS Manager] Using native playback for:', streamUrl);
            // Cleanup any previous HLS instance before native playback
            this.detach(elementId);
            element.src = streamUrl;
            return { type: 'native', url: streamUrl };
        }

        // Check native HLS support (Safari, iOS)
        if (element.canPlayType('application/vnd.apple.mpegurl')) {
            console.log('[HLS Manager] Using native HLS support');
            // Cleanup any previous HLS instance before native-HLS playback
            this.detach(elementId);
            element.src = streamUrl;
            return { type: 'native-hls', url: streamUrl };
        }

        // Generate unique attach token to prevent race conditions
        const attachId = Symbol();
        this.activeAttachMap.set(elementId, attachId);

        // Return a Promise that will be settled via settlePendingAttach
        return new Promise((resolve, reject) => {
            // Check if there's an existing pending resolve for this elementId
            const existing = this.pendingResolves.get(elementId);
            if (existing && existing.attachId !== attachId) {
                // Cancel the old pending promise by resolving it as cancelled
                existing.resolve({ type: 'cancelled', url: streamUrl });
                this.pendingResolves.delete(elementId);
            }

            // Store the resolve/reject with attachId (it will be called by settlePendingAttach)
            this.pendingResolves.set(elementId, { attachId, resolve, reject });

            // Start async work only after the real resolver is registered
            this._doAttach(elementId, streamUrl, attachId).catch((e) => {
                console.error('[HLS Manager] Attach failed:', e);
                // Ensure promise is settled even on unexpected errors
                if (this.activeAttachMap.get(elementId) === attachId) {
                    this.settlePendingAttach(elementId, {
                        type: 'error',
                        url: streamUrl,
                        error: e.message || 'Unknown error'
                    }, attachId);
                }
            });
        });
    },

    /**
     * Internal async method to perform the actual attach work
     * Separated from Promise executor to properly handle exceptions
     */
    async _doAttach(elementId, streamUrl, attachId) {
        const element = document.getElementById(elementId);
        if (!element || !(element instanceof HTMLMediaElement)) {
            this.activeAttachMap.delete(elementId);
            this.settlePendingAttach(elementId, {
                type: 'error',
                url: streamUrl,
                error: 'Media element not found'
            }, attachId);
            return;
        }

        // Use hls.js for browsers without native HLS support
        try {
            await this.loadHls();
        } catch (e) {
            if (this.activeAttachMap.get(elementId) !== attachId) {
                this.settlePendingAttach(elementId, { type: 'cancelled', url: streamUrl }, attachId);
                return;
            }
            // Remove only our own token to avoid clobbering a newer attach
            this.activeAttachMap.delete(elementId);
            console.error('[HLS Manager] Failed to load HLS:', e);
            this.settlePendingAttach(elementId, {
                type: 'error',
                url: streamUrl,
                error: 'Failed to load hls.js'
            }, attachId);
            return;
        }

        // Check if this attach is still valid after await
        if (this.activeAttachMap.get(elementId) !== attachId) {
            this.settlePendingAttach(elementId, { type: 'cancelled', url: streamUrl }, attachId);
            return;
        }

        if (!Hls.isSupported()) {
            this.activeAttachMap.delete(elementId);
            this.settlePendingAttach(elementId, {
                type: 'error',
                url: streamUrl,
                error: 'HLS not supported in this browser'
            }, attachId);
            return;
        }

        // Cleanup any previous instance before creating new one (use internal cleanup to not settle promise)
        this.cleanupInstance(elementId);

        // Re-register attach token after cleanup to maintain valid attach state
        this.activeAttachMap.set(elementId, attachId);

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

        // Track instance immediately to prevent orphaned workers on timeout
        this.instances.set(elementId, hls);

        // Check if this attach is still valid before setting up
        if (this.activeAttachMap.get(elementId) !== attachId) {
            hls.destroy();
            this.instances.delete(elementId);
            this.settlePendingAttach(elementId, { type: 'cancelled', url: streamUrl }, attachId);
            return;
        }

        // Capture the current attachId for timeout validation
        const capturedAttachId = attachId;
        const timeout = setTimeout(() => {
            // Verify this timeout belongs to the current attach
            if (this.activeAttachMap.get(elementId) !== capturedAttachId) {
                return;
            }
            this.pendingTimeouts.delete(elementId);
            this.instances.delete(elementId);
            this.activeAttachMap.delete(elementId);
            hls.destroy();
            this.settlePendingAttach(elementId, {
                type: 'error',
                url: streamUrl,
                error: 'HLS stream timeout - stream may be offline'
            }, capturedAttachId);
        }, 15000);
        this.pendingTimeouts.set(elementId, timeout);

        hls.attachMedia(element);

        hls.on(Hls.Events.MEDIA_ATTACHED, () => {
            // Check if this attach is still valid
            if (this.activeAttachMap.get(elementId) !== attachId) {
                clearTimeout(timeout);
                hls.destroy();
                return;
            }
            console.log('[HLS Manager] Media attached, loading source:', streamUrl);
            hls.loadSource(streamUrl);
        });

        hls.on(Hls.Events.MANIFEST_PARSED, (event, data) => {
            // Check if this attach is still valid
            if (this.activeAttachMap.get(elementId) !== attachId) {
                clearTimeout(timeout);
                hls.destroy();
                return;
            }
            clearTimeout(timeout);
            this.pendingTimeouts.delete(elementId);
            console.log('[HLS Manager] Manifest parsed, levels:', data.levels.length);
            // Instance already tracked above; no need to set again
            this.settlePendingAttach(elementId, {
                type: 'success',
                backend: 'hls.js',
                levels: data.levels.length,
                url: streamUrl
            }, attachId);
        });

        hls.on(Hls.Events.ERROR, (event, data) => {
            // Validate this error belongs to the current attach
            if (this.activeAttachMap.get(elementId) !== attachId) {
                return;
            }
            if (data.fatal) {
                clearTimeout(timeout);
                this.pendingTimeouts.delete(elementId);
                console.error('[HLS Manager] Fatal error:', data.type, data.details);
                hls.destroy();
                this.instances.delete(elementId);
                this.activeAttachMap.delete(elementId);
                if (this.nowPlayingElementId === elementId) {
                    this.nowPlaying = null;
                    this.nowPlayingElementId = null;
                }

                // Dispatch error event for Rust to catch
                window.dispatchEvent(new CustomEvent('hls-stream-error', {
                    detail: {
                        elementId: elementId,
                        error: data.details,
                        type: data.type
                    }
                }));

                this.settlePendingAttach(elementId, {
                    type: 'error',
                    url: streamUrl,
                    error: 'HLS error: ' + data.details
                }, attachId);
            } else {
                // Non-fatal error - hls.js will try to recover
                console.warn('[HLS Manager] Non-fatal error:', data.details);
            }
        });

        // Listen for ID3 timed metadata (now-playing info in some HLS streams)
        hls.on(Hls.Events.FRAG_PARSING_METADATA, (event, data) => {
            // Guard against stale metadata from older Hls instances
            if (this.activeAttachMap.get(elementId) !== attachId) {
                return;
            }
            const samples = data.samples || [];
            samples.forEach(sample => {
                try {
                    const frames = this.parseId3(sample.data);
                    if (frames && (frames.TIT2 || frames.TPE1)) {
                        // Store in hlsManager for polling from Rust
                        this.nowPlayingElementId = elementId;
                        this.nowPlaying = {
                            title: frames.TIT2 || null,
                            artist: frames.TPE1 || null
                        };
                        console.log('[HLS Manager] Now playing:', this.nowPlaying);

                        // Also dispatch event for any JS listeners
                        window.dispatchEvent(new CustomEvent('hls-metadata', {
                            detail: {
                                elementId: elementId,
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
    },

    /**
     * Internal cleanup method that removes resources without settling promises
     * Used during attach flow to avoid prematurely settling the public attach Promise
     */
    cleanupInstance(elementId) {
        // Clear timeouts but DON'T settle pending promises
        const pendingTimeout = this.pendingTimeouts.get(elementId);
        if (pendingTimeout) {
            clearTimeout(pendingTimeout);
            this.pendingTimeouts.delete(elementId);
        }

        // Clear active attach token
        this.activeAttachMap.delete(elementId);

        // Destroy HLS instance if exists
        const hls = this.instances.get(elementId);
        if (hls) {
            console.log('[HLS Manager] Destroying HLS instance for:', elementId);
            hls.destroy();
            this.instances.delete(elementId);
        }

        // Clear now playing metadata only if this audio owned it
        if (this.nowPlayingElementId === elementId) {
            this.nowPlaying = null;
            this.nowPlayingElementId = null;
        }
    },

    /**
     * Detach and cleanup HLS instance (public API - settles pending promises)
     */
    detach(elementId) {
        const pending = this.pendingResolves.get(elementId);
        const pendingAttachId = pending ? pending.attachId : null;

        this.clearPendingAttach(elementId, false);

        // Clear active attach token to invalidate pending operations
        this.activeAttachMap.delete(elementId);

        // Resolve any in-flight attach Promise as cancelled
        if (pendingAttachId) {
            this.settlePendingAttach(elementId, { type: 'cancelled' }, pendingAttachId);
        } else {
            // No pending attach, clean up the entry
            this.pendingResolves.delete(elementId);
        }

        const hls = this.instances.get(elementId);
        if (hls) {
            console.log('[HLS Manager] Destroying HLS instance for:', elementId);
            hls.destroy();
            this.instances.delete(elementId);
        }
        // Clear now playing metadata only if this audio owned it
        if (this.nowPlayingElementId === elementId) {
            this.nowPlaying = null;
            this.nowPlayingElementId = null;
        }
    },

    /**
     * Get available quality levels for HLS stream
     */
    getQualityLevels(elementId) {
        const hls = this.instances.get(elementId);
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
    setQualityLevel(elementId, levelIndex) {
        const hls = this.instances.get(elementId);
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
