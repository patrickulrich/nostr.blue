package dev.dioxus.main

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.MediaMetadata
import android.media.MediaPlayer
import android.media.PlaybackParams
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.media.app.NotificationCompat.MediaStyle
import org.json.JSONArray
import org.json.JSONObject
import java.lang.ref.WeakReference
import java.util.concurrent.atomic.AtomicReference

private const val AUDIO_TAG = "NativeAudio"
private const val CHANNEL_ID = "nostrblue_media"
private const val NOTIFICATION_ID = 7001

data class NativeQueueItem(
    val id: String,
    val title: String,
    val artist: String,
    val album: String?,
    val mediaUrl: String,
    val albumArtUrl: String?,
    val durationSeconds: Double?,
    val isLiveStream: Boolean,
    val isPodcast: Boolean
)

object NativeAudioBridge {
    const val ACTION_PLAY = "com.nostr.blue.media.PLAY"
    const val ACTION_PAUSE = "com.nostr.blue.media.PAUSE"
    const val ACTION_NEXT = "com.nostr.blue.media.NEXT"
    const val ACTION_PREVIOUS = "com.nostr.blue.media.PREVIOUS"
    const val ACTION_STOP = "com.nostr.blue.media.STOP"

    private val queue = mutableListOf<NativeQueueItem>()
    private var player: MediaPlayer? = null
    private var mediaSession: MediaSession? = null
    private var currentIndex: Int = 0
    private var playWhenReady: Boolean = false
    private var isPreparing: Boolean = false
    private var lastDurationSeconds: Double = 0.0
    private val lastError = AtomicReference<String?>(null)
    private var appContext: Context? = null
    private var serviceRef: WeakReference<MediaPlaybackService>? = null

    private data class ParsedQueueResult(
        val items: List<NativeQueueItem>,
        val adjustedStartIndex: Int
    )

    @Synchronized
    fun attachService(service: MediaPlaybackService) {
        serviceRef = WeakReference(service)
        ensureInitialized(service.applicationContext)
        updateNotification()
    }

    @Synchronized
    fun detachService(service: MediaPlaybackService) {
        if (serviceRef?.get() === service) {
            serviceRef = null
        }
    }

    @Synchronized
    fun ensureInitialized(context: Context) {
        val applicationContext = context.applicationContext
        appContext = applicationContext
        ensureChannel(applicationContext)
        if (mediaSession == null) {
            mediaSession = MediaSession(applicationContext, "nostrblue-media").apply {
                setCallback(
                    object : MediaSession.Callback() {
                        override fun onPlay() {
                            play(applicationContext)
                        }

                        override fun onPause() {
                            pause(applicationContext)
                        }

                        override fun onSkipToNext() {
                            skipNext(applicationContext)
                        }

                        override fun onSkipToPrevious() {
                            skipPrevious(applicationContext)
                        }

                        override fun onSeekTo(pos: Long) {
                            seekTo(applicationContext, pos)
                        }
                    },
                    Handler(Looper.getMainLooper())
                )
                isActive = true
            }
        }
    }

    @Synchronized
    fun ensureServiceStarted(context: Context) {
        ensureInitialized(context)
        val intent = Intent(context, MediaPlaybackService::class.java)
        ContextCompat.startForegroundService(context, intent)
    }

    @Synchronized
    fun setQueue(context: Context, queueJson: String, startIndex: Int, playWhenReady: Boolean): String {
        return try {
            val parsed = parseQueue(queueJson, startIndex)
            queue.clear()
            queue.addAll(parsed.items)
            currentIndex = parsed.adjustedStartIndex.coerceIn(0, (queue.size - 1).coerceAtLeast(0))
            this.playWhenReady = playWhenReady
            lastError.set(null)
            if (queue.isEmpty()) {
                resetSnapshotState()
                releasePlayer()
                updatePlaybackState(false, PlaybackState.STATE_STOPPED)
                stopForegroundPlayback()
            } else {
                ensureServiceStarted(context)
                prepareCurrent(playWhenReady)
            }
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "setQueue failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun play(context: Context): String {
        return try {
            if (queue.isEmpty()) return "ok"
            ensureServiceStarted(context)
            if (isPreparing) {
                playWhenReady = true
                updatePlaybackState(false, PlaybackState.STATE_BUFFERING)
                updateNotification()
                return "ok"
            }
            if (player == null) {
                prepareCurrent(true)
                return "ok"
            }
            val player = ensurePlayer()
            if (!player.isPlaying) {
                player.start()
            }
            playWhenReady = true
            updatePlaybackState(true, PlaybackState.STATE_PLAYING)
            updateNotification()
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "play failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun pause(context: Context): String {
        return try {
            ensureInitialized(context)
            playWhenReady = false
            player?.takeIf { it.isPlaying }?.pause()
            updatePlaybackState(false, PlaybackState.STATE_PAUSED)
            updateNotification()
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "pause failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun stop(context: Context): String {
        return try {
            ensureInitialized(context)
            playWhenReady = false
            releasePlayer()
            updatePlaybackState(false, PlaybackState.STATE_STOPPED)
            stopForegroundPlayback()
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "stop failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun skipNext(context: Context): String {
        return try {
            ensureInitialized(context)
            if (queue.isEmpty()) return "ok"
            currentIndex = (currentIndex + 1).mod(queue.size)
            prepareCurrent(playWhenReady)
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "skipNext failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun skipPrevious(context: Context): String {
        return try {
            ensureInitialized(context)
            if (queue.isEmpty()) return "ok"
            val currentPosition = if (isPreparing) 0 else (player?.currentPosition ?: 0)
            if (currentPosition > 3_000) {
                player?.seekTo(0)
            } else {
                currentIndex = if (currentIndex == 0) queue.lastIndex else currentIndex - 1
                prepareCurrent(playWhenReady)
            }
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "skipPrevious failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun seekTo(context: Context, positionMs: Long): String {
        return try {
            ensureInitialized(context)
            val targetMs = positionMs.coerceAtLeast(0L).coerceAtMost(Int.MAX_VALUE.toLong()).toInt()
            player?.seekTo(targetMs)
            updatePlaybackState(player?.isPlaying == true, currentPlaybackState())
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "seekTo failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun setPlaybackSpeed(context: Context, speed: Float): String {
        return try {
            ensureInitialized(context)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                player?.let { mediaPlayer ->
                    val params = (mediaPlayer.playbackParams ?: PlaybackParams()).setSpeed(speed)
                    mediaPlayer.playbackParams = params
                }
            }
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "setPlaybackSpeed failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun setVolume(context: Context, volume: Float): String {
        return try {
            ensureInitialized(context)
            player?.setVolume(volume.coerceIn(0f, 1f), volume.coerceIn(0f, 1f))
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "setVolume failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun clearQueue(context: Context): String {
        return try {
            ensureInitialized(context)
            queue.clear()
            resetSnapshotState()
            releasePlayer()
            updatePlaybackState(false, PlaybackState.STATE_STOPPED)
            stopForegroundPlayback()
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "clearQueue failed", e)
            "error:${e.message}"
        }
    }

    @Synchronized
    fun getSnapshot(context: Context): String {
        return try {
            ensureInitialized(context)
            val activelyPlayingOrStarting = player?.isPlaying == true || (isPreparing && playWhenReady)
            val currentPositionSeconds = safeCurrentPositionSeconds()
            val durationSeconds = safeDurationSeconds()
            val obj = JSONObject().apply {
                put("queue_len", queue.size)
                put("current_index", currentIndex)
                put("is_playing", activelyPlayingOrStarting)
                put("is_buffering", isPreparing)
                put("current_time", currentPositionSeconds)
                put("duration", durationSeconds)
                put("playback_error", lastError.get())
            }
            obj.toString()
        } catch (e: Exception) {
            JSONObject().put("playback_error", e.message ?: "snapshot_failed").toString()
        }
    }

    private fun ensurePlayer(): MediaPlayer {
        val current = player
        if (current != null) {
            return current
        }
        return MediaPlayer().apply {
            setAudioAttributes(
                AudioAttributes.Builder()
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .build()
            )
            setOnPreparedListener {
                synchronized(this@NativeAudioBridge) {
                    isPreparing = false
                    lastDurationSeconds = if (it.duration > 0) it.duration / 1000.0 else 0.0
                    if (playWhenReady) {
                        it.start()
                        updatePlaybackState(true, PlaybackState.STATE_PLAYING)
                    } else {
                        updatePlaybackState(false, PlaybackState.STATE_PAUSED)
                    }
                    updateNotification()
                }
            }
            setOnCompletionListener {
                synchronized(this@NativeAudioBridge) {
                    isPreparing = false
                    if (queue.isEmpty()) {
                        playWhenReady = false
                        releasePlayer()
                        updatePlaybackState(false, PlaybackState.STATE_STOPPED)
                        stopForegroundPlayback()
                        // Clear service state before updating notification to prevent re-triggering startForeground()
                        serviceRef = null
                        updateNotification()
                        return@synchronized
                    }
                    currentIndex = (currentIndex + 1).mod(queue.size)
                    prepareCurrent(true)
                }
            }
            setOnErrorListener { _, _, _ ->
                synchronized(this@NativeAudioBridge) {
                    isPreparing = false
                    playWhenReady = false
                    lastError.set("Playback failed")
                    releasePlayer()
                    updatePlaybackState(false, PlaybackState.STATE_ERROR)
                    updateNotification()
                }
                true
            }
            player = this
        }
    }

    @Synchronized
    private fun prepareCurrent(playWhenReady: Boolean) {
        val item = queue.getOrNull(currentIndex) ?: return
        this.playWhenReady = playWhenReady
        isPreparing = true
        // Reset per-track state before preparing new track
        lastError.set(null)
        lastDurationSeconds = 0.0
        val player = ensurePlayer()
        player.reset()
        player.setAudioAttributes(
            AudioAttributes.Builder()
                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .build()
        )
        try {
            player.setDataSource(item.mediaUrl)
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "Failed to load media source for item=${item.id}", e)
            releasePlayer()
            lastError.set("Failed to load media source")
            isPreparing = false
            updatePlaybackState(false, PlaybackState.STATE_ERROR)
            updateNotification()
            return
        }
        updatePlaybackState(false, PlaybackState.STATE_BUFFERING)
        updateMetadata(item)
        updateNotification()
        player.prepareAsync()
    }

    @Synchronized
    private fun releasePlayer() {
        isPreparing = false
        player?.release()
        player = null
    }

    private fun resetSnapshotState() {
        currentIndex = 0
        playWhenReady = false
        lastDurationSeconds = 0.0
        lastError.set(null)
    }

    private fun updateMetadata(item: NativeQueueItem) {
        val metadata = MediaMetadata.Builder()
            .putString(MediaMetadata.METADATA_KEY_TITLE, item.title)
            .putString(MediaMetadata.METADATA_KEY_ARTIST, item.artist)
            .putString(MediaMetadata.METADATA_KEY_ALBUM, item.album)
            .build()
        mediaSession?.setMetadata(metadata)
    }

    private fun updatePlaybackState(isPlaying: Boolean, state: Int) {
        val actions = PlaybackState.ACTION_PLAY or
            PlaybackState.ACTION_PAUSE or
            PlaybackState.ACTION_PLAY_PAUSE or
            PlaybackState.ACTION_SKIP_TO_NEXT or
            PlaybackState.ACTION_SKIP_TO_PREVIOUS or
            PlaybackState.ACTION_SEEK_TO or
            PlaybackState.ACTION_STOP
        val positionMs = if (isPreparing) {
            0L
        } else {
            try {
                (player?.currentPosition ?: 0).toLong()
            } catch (_: IllegalStateException) {
                0L
            }
        }
        val playbackState = PlaybackState.Builder()
            .setActions(actions)
            .setState(state, positionMs, if (isPlaying) 1.0f else 0.0f)
            .build()
        mediaSession?.setPlaybackState(playbackState)
        mediaSession?.isActive = true
    }

    private fun currentPlaybackState(): Int {
        return when {
            player == null -> PlaybackState.STATE_STOPPED
            isPreparing -> PlaybackState.STATE_BUFFERING
            player?.isPlaying == true -> PlaybackState.STATE_PLAYING
            else -> PlaybackState.STATE_PAUSED
        }
    }

    private fun safeCurrentPositionSeconds(): Double {
        val activePlayer = player ?: return 0.0
        if (isPreparing) {
            return 0.0
        }
        return try {
            activePlayer.currentPosition.toDouble() / 1000.0
        } catch (_: IllegalStateException) {
            0.0
        }
    }

    private fun safeDurationSeconds(): Double {
        if (isPreparing) {
            return lastDurationSeconds
        }
        val activePlayer = player ?: return lastDurationSeconds
        return try {
            activePlayer.duration
                .takeIf { it > 0 }
                ?.toDouble()
                ?.div(1000.0)
                ?: lastDurationSeconds
        } catch (_: IllegalStateException) {
            lastDurationSeconds
        }
    }

    private fun updateNotification() {
        val context = appContext ?: return
        val service = serviceRef?.get()
        val item = queue.getOrNull(currentIndex)
        if (service == null || item == null) {
            return
        }
        val builder = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle(item.title)
            .setContentText(item.artist)
            .setStyle(
                MediaStyle()
                    .setMediaSession(mediaSession?.sessionToken)
                    .setShowActionsInCompactView(0, 1, 2)
            )
            .setOnlyAlertOnce(true)
            .setOngoing(player?.isPlaying == true || (isPreparing && playWhenReady))
            .addAction(
                android.R.drawable.ic_media_previous,
                "Previous",
                serviceIntent(context, ACTION_PREVIOUS)
            )
            .addAction(
                if (player?.isPlaying == true || (isPreparing && playWhenReady)) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
                if (player?.isPlaying == true || (isPreparing && playWhenReady)) "Pause" else "Play",
                serviceIntent(
                    context,
                    if (player?.isPlaying == true || (isPreparing && playWhenReady)) ACTION_PAUSE else ACTION_PLAY
                )
            )
            .addAction(
                android.R.drawable.ic_media_next,
                "Next",
                serviceIntent(context, ACTION_NEXT)
            )

        service.startForeground(NOTIFICATION_ID, builder.build())
    }

    private fun stopForegroundPlayback() {
        val service = serviceRef?.get() ?: return
        service.stopForeground(Service.STOP_FOREGROUND_REMOVE)
        NotificationManagerCompat.from(service).cancel(NOTIFICATION_ID)
        service.stopSelf()
    }

    private fun serviceIntent(context: Context, action: String): PendingIntent {
        val intent = Intent(context, MediaPlaybackService::class.java).setAction(action)
        return PendingIntent.getService(
            context,
            action.hashCode(),
            intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
    }

    private fun ensureChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return
        }
        val manager = context.getSystemService(NotificationManager::class.java)
        if (manager.getNotificationChannel(CHANNEL_ID) == null) {
            manager.createNotificationChannel(
                NotificationChannel(
                    CHANNEL_ID,
                    "Media playback",
                    NotificationManager.IMPORTANCE_LOW
                )
            )
        }
    }

    private fun parseQueue(queueJson: String, startIndex: Int): ParsedQueueResult {
        val arr = JSONArray(queueJson)
        var adjustedStartIndex = startIndex
        val items = buildList(arr.length()) {
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                val mediaUrl = obj.optString("media_url")
                // Skip entries with blank or missing media_url
                if (mediaUrl.isBlank()) {
                    if (i < startIndex) {
                        adjustedStartIndex -= 1
                    }
                    continue
                }
                add(
                    NativeQueueItem(
                        id = obj.optString("id"),
                        title = obj.optString("title"),
                        artist = obj.optString("artist"),
                        album = obj.optString("album").takeUnless { it.isBlank() },
                        mediaUrl = mediaUrl,
                        albumArtUrl = obj.optString("album_art_url").takeUnless { it.isBlank() },
                        durationSeconds = obj.optDouble("duration").takeUnless { it.isNaN() || it <= 0.0 },
                        isLiveStream = obj.optBoolean("is_live_stream", false),
                        isPodcast = obj.optBoolean("is_podcast", false)
                    )
                )
            }
        }
        return ParsedQueueResult(
            items = items,
            adjustedStartIndex = adjustedStartIndex.coerceAtLeast(0)
        )
    }
}
