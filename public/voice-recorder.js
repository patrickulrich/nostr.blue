// Simple voice recorder implementation
// Handles all MediaRecorder complexity in JavaScript

class VoiceRecorderManager {
    constructor() {
        this.recorders = new Map();
    }

    async startRecording(recorderId) {
        try {
            // Clean up any existing recorder with this ID
            if (this.recorders.has(recorderId)) {
                this.stopRecording(recorderId);
            }

            // Request microphone access
            const stream = await navigator.mediaDevices.getUserMedia({ audio: true });

            // Try MIME types in order of preference
            const mimeTypes = [
                'audio/webm;codecs=opus',
                'audio/webm',
                'audio/mp4',
                'audio/ogg;codecs=opus'
            ];

            let selectedMime = 'audio/webm'; // fallback
            for (const mime of mimeTypes) {
                if (MediaRecorder.isTypeSupported(mime)) {
                    selectedMime = mime;
                    console.log('[VoiceRecorder] Selected MIME type:', selectedMime);
                    break;
                }
            }

            // Create MediaRecorder with timeslice for regular data chunks
            const recorder = new MediaRecorder(stream, { mimeType: selectedMime });
            const chunks = [];
            const startTime = Date.now();

            recorder.ondataavailable = (e) => {
                if (e.data && e.data.size > 0) {
                    chunks.push(e.data);
                    console.log('[VoiceRecorder] Chunk recorded:', e.data.size, 'bytes, total chunks:', chunks.length);
                }
            };

            recorder.onstop = async () => {
                console.log('[VoiceRecorder] Recording stopped, total chunks:', chunks.length);

                const duration = (Date.now() - startTime) / 1000;
                const blob = new Blob(chunks, { type: selectedMime });

                // Convert blob to array buffer for Rust
                const arrayBuffer = await blob.arrayBuffer();
                const bytes = new Uint8Array(arrayBuffer);

                // Extract waveform
                const waveform = await this.extractWaveform(blob);

                // Store result
                const result = {
                    bytes: bytes,
                    duration: duration,
                    waveform: waveform,
                    mimeType: selectedMime,
                    success: true
                };

                this.recorders.set(recorderId, { result, stream, recorder: null });

                // Stop all tracks
                stream.getTracks().forEach(track => track.stop());

                console.log('[VoiceRecorder] Recording ready:', duration.toFixed(2) + 's', 'size:', bytes.length, 'bytes');
            };

            recorder.onerror = (e) => {
                console.error('[VoiceRecorder] Error:', e);
                this.recorders.set(recorderId, {
                    result: { success: false, error: e.message || 'Recording error' },
                    stream,
                    recorder: null
                });
            };

            // Start recording with 1 second timeslice
            recorder.start(1000);

            this.recorders.set(recorderId, { recorder, stream, chunks, startTime });

            console.log('[VoiceRecorder] Started recording with ID:', recorderId);
            return { success: true };

        } catch (error) {
            console.error('[VoiceRecorder] Failed to start:', error);
            return { success: false, error: error.message };
        }
    }

    stopRecording(recorderId) {
        const data = this.recorders.get(recorderId);
        if (!data) {
            console.warn('[VoiceRecorder] No recorder found for ID:', recorderId);
            return { success: false, error: 'Recorder not found' };
        }

        if (data.recorder && data.recorder.state !== 'inactive') {
            console.log('[VoiceRecorder] Stopping recorder:', recorderId);
            data.recorder.stop();
        }

        return { success: true };
    }

    getResult(recorderId) {
        const data = this.recorders.get(recorderId);
        if (!data || !data.result) {
            return null;
        }
        return data.result;
    }

    cleanup(recorderId) {
        const data = this.recorders.get(recorderId);
        if (data) {
            if (data.stream) {
                data.stream.getTracks().forEach(track => track.stop());
            }
            if (data.recorder && data.recorder.state !== 'inactive') {
                data.recorder.stop();
            }
        }
        this.recorders.delete(recorderId);
        console.log('[VoiceRecorder] Cleaned up recorder:', recorderId);
    }

    async extractWaveform(blob) {
        try {
            const arrayBuffer = await blob.arrayBuffer();
            const audioContext = new (window.OfflineAudioContext || window.webkitOfflineAudioContext)(1, 1, 44100);
            const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

            const samples = audioBuffer.getChannelData(0);
            const numBuckets = 100;
            const bucketSize = Math.floor(samples.length / numBuckets);

            const waveform = [];

            for (let i = 0; i < numBuckets; i++) {
                const start = i * bucketSize;
                const end = Math.min(start + bucketSize, samples.length);

                let sumSquares = 0;
                for (let j = start; j < end; j++) {
                    sumSquares += samples[j] * samples[j];
                }
                const rms = Math.sqrt(sumSquares / (end - start));
                const normalized = Math.min(100, Math.floor(rms * 200));
                waveform.push(normalized);
            }

            console.log('[VoiceRecorder] Extracted waveform, peak:', Math.max(...waveform));
            return waveform;
        } catch (err) {
            console.warn('[VoiceRecorder] Failed to extract waveform:', err.message);
            return Array(100).fill(0);
        }
    }
}

// Global instance
window.voiceRecorderManager = new VoiceRecorderManager();
