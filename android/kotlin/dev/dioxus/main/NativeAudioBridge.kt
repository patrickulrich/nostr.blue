package dev.dioxus.main

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaLibraryService
import androidx.media3.session.MediaSession
import androidx.media3.session.SessionError
import androidx.media.utils.MediaConstants
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject
import java.lang.ref.WeakReference
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicReference

private const val AUDIO_TAG = "NativeAudio"

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
    private var player: ExoPlayer? = null
    @Volatile
    var mediaLibrarySession: MediaLibraryService.MediaLibrarySession? = null
        private set
    private var currentIndex: Int = 0
    private var playWhenReady: Boolean = false
    private val lastError = AtomicReference<String?>(null)
    private var appContext: Context? = null
    private var serviceRef: WeakReference<MediaPlaybackService>? = null
    private val mainHandler = Handler(Looper.getMainLooper())

    private fun <T> runOnMainThread(block: () -> T): T {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return block()
        }
        var result: T? = null
        var exception: Exception? = null
        val latch = CountDownLatch(1)
        mainHandler.post {
            try {
                result = block()
            } catch (e: Exception) {
                exception = e
            } finally {
                latch.countDown()
            }
        }
        latch.await()
        exception?.let { throw it }
        @Suppress("UNCHECKED_CAST")
        return result as T
    }

    private data class ParsedQueueResult(
        val items: List<NativeQueueItem>,
        val adjustedStartIndex: Int
    )

    @Synchronized
    fun attachService(service: MediaPlaybackService) {
        serviceRef = WeakReference(service)
        ensureInitialized(service.applicationContext)
        ensurePlayer()
        mediaLibrarySession?.let { service.addSession(it) }
    }

    @Synchronized
    fun detachService(service: MediaPlaybackService) {
        if (serviceRef?.get() === service) {
            serviceRef = null
        }
    }

    @Synchronized
    fun ensureInitialized(context: Context) {
        if (appContext == null) {
            appContext = context.applicationContext
        }
    }

    private fun ensureServiceStarted(context: Context) {
        ensureInitialized(context)
        try {
            val intent = Intent(context, MediaPlaybackService::class.java)
            ContextCompat.startForegroundService(context, intent)
        } catch (_: Exception) {}
    }

    fun setQueue(context: Context, queueJson: String, startIndex: Int, playWhenReady: Boolean): String {
        return runOnMainThread {
            try {
                val parsed = parseQueue(queueJson, startIndex)
                queue.clear()
                queue.addAll(parsed.items)
                currentIndex = parsed.adjustedStartIndex.coerceIn(0, (queue.size - 1).coerceAtLeast(0))
                this.playWhenReady = playWhenReady
                lastError.set(null)
                if (queue.isEmpty()) {
                    resetSnapshotState()
                    releasePlayer()
                } else {
                    ensureServiceStarted(context)
                    prepareCurrent(this.playWhenReady)
                }
                "ok"
            } catch (e: Exception) {
                Log.e(AUDIO_TAG, "setQueue failed", e)
                "error:${e.message}"
            }
        }
    }

    fun play(context: Context): String = runOnMainThread {
        try {
            if (queue.isEmpty()) return@runOnMainThread "ok"
            ensureServiceStarted(context)
            val p = player
            if (p == null) {
                prepareCurrent(true)
                return@runOnMainThread "ok"
            }
            if (!p.isPlaying) {
                p.play()
            }
            playWhenReady = true
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "play failed", e)
            "error:${e.message}"
        }
    }

    fun pause(context: Context): String = runOnMainThread {
        try {
            ensureInitialized(context)
            playWhenReady = false
            player?.takeIf { it.isPlaying }?.pause()
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "pause failed", e)
            "error:${e.message}"
        }
    }

    fun stop(context: Context): String = runOnMainThread {
        try {
            ensureInitialized(context)
            playWhenReady = false
            releasePlayer()
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "stop failed", e)
            "error:${e.message}"
        }
    }

    fun skipNext(context: Context): String = runOnMainThread {
        try {
            ensureInitialized(context)
            if (queue.isEmpty()) return@runOnMainThread "ok"
            player?.seekToNext()
            currentIndex = player?.currentMediaItemIndex ?: currentIndex
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "skipNext failed", e)
            "error:${e.message}"
        }
    }

    fun skipPrevious(context: Context): String = runOnMainThread {
        try {
            ensureInitialized(context)
            if (queue.isEmpty()) return@runOnMainThread "ok"
            val currentPosition = player?.currentPosition ?: 0
            if (currentPosition > 3_000) {
                player?.seekTo(0)
            } else {
                player?.seekToPrevious()
            }
            currentIndex = player?.currentMediaItemIndex ?: currentIndex
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "skipPrevious failed", e)
            "error:${e.message}"
        }
    }

    fun seekTo(context: Context, positionMs: Long): String = runOnMainThread {
        try {
            ensureInitialized(context)
            val targetMs = positionMs.coerceAtLeast(0L)
            player?.seekTo(targetMs)
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "seekTo failed", e)
            "error:${e.message}"
        }
    }

    fun setPlaybackSpeed(context: Context, speed: Float): String = runOnMainThread {
        try {
            ensureInitialized(context)
            player?.setPlaybackSpeed(speed)
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "setPlaybackSpeed failed", e)
            "error:${e.message}"
        }
    }

    fun setVolume(context: Context, volume: Float): String = runOnMainThread {
        try {
            ensureInitialized(context)
            player?.setVolume(volume.coerceIn(0f, 1f))
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "setVolume failed", e)
            "error:${e.message}"
        }
    }

    fun clearQueue(context: Context): String = runOnMainThread {
        try {
            ensureInitialized(context)
            queue.clear()
            resetSnapshotState()
            releasePlayer()
            "ok"
        } catch (e: Exception) {
            Log.e(AUDIO_TAG, "clearQueue failed", e)
            "error:${e.message}"
        }
    }

    fun getSnapshot(context: Context): String = runOnMainThread {
        try {
            ensureInitialized(context)
            val p = player
            val snapshotIsPlaying = p?.isPlaying == true || (p?.playbackState == Player.STATE_BUFFERING && playWhenReady)
            val snapshotIsBuffering = p?.playbackState == Player.STATE_BUFFERING
            val snapshotCurrentTime = (p?.currentPosition ?: 0) / 1000.0
            val snapshotDuration = (p?.duration?.takeIf { it > 0 } ?: 0) / 1000.0
            JSONObject().apply {
                put("queue_len", queue.size)
                put("current_index", p?.currentMediaItemIndex ?: currentIndex)
                put("is_playing", snapshotIsPlaying)
                put("is_buffering", snapshotIsBuffering)
                put("current_time", snapshotCurrentTime)
                put("duration", snapshotDuration)
                put("playback_error", lastError.get())
            }.toString()
        } catch (e: Exception) {
            JSONObject().put("playback_error", e.message ?: "snapshot_failed").toString()
        }
    }

    @Suppress("UnstableApiUsage")
    private fun ensurePlayer(): ExoPlayer {
        player?.let { return it }
        val context = appContext ?: throw IllegalStateException("Not initialized")
        val service = serviceRef?.get()
            ?: throw IllegalStateException("Service not attached")
        val audioAttributes = AudioAttributes.Builder()
            .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
            .setUsage(C.USAGE_MEDIA)
            .build()
        val p = ExoPlayer.Builder(context)
            .setAudioAttributes(audioAttributes, true)
            .setHandleAudioBecomingNoisy(true)
            .build()
        p.addListener(playerListener)
        player = p

        mediaLibrarySession?.release()
        mediaLibrarySession = MediaLibraryService.MediaLibrarySession.Builder(
            service, p, libraryCallback
        ).setSessionActivity(
            PendingIntent.getActivity(
                context, 0,
                Intent(context, Class.forName("dev.dioxus.main.MainActivity")),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
        ).build()
        serviceRef?.get()?.addSession(mediaLibrarySession!!)

        return p
    }

    private val playerListener = object : Player.Listener {
        override fun onPlaybackStateChanged(playbackState: Int) {
            if (playbackState == Player.STATE_ENDED) {
                playWhenReady = false
                player?.stop()
            }
        }

        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
            player?.currentMediaItemIndex?.let { idx ->
                if (idx != currentIndex && idx in queue.indices) {
                    currentIndex = idx
                }
            }
        }

        override fun onPlayerError(error: PlaybackException) {
            Log.e(AUDIO_TAG, "Player error: ${error.message}", error)
            lastError.set("Playback failed: ${error.message}")
            playWhenReady = false
        }
    }

    @Suppress("UnstableApiUsage")
    private val libraryCallback = object : MediaLibraryService.MediaLibrarySession.Callback {
        override fun onConnect(
            session: MediaSession,
            controller: MediaSession.ControllerInfo
        ): MediaSession.ConnectionResult {
            val sessionCommands = MediaSession.ConnectionResult.DEFAULT_SESSION_AND_LIBRARY_COMMANDS
            return MediaSession.ConnectionResult.AcceptedResultBuilder(session)
                .setAvailableSessionCommands(sessionCommands)
                .setAvailablePlayerCommands(Player.Commands.Builder().addAllCommands().build())
                .build()
        }

        override fun onGetLibraryRoot(
            session: MediaLibraryService.MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            params: MediaLibraryService.LibraryParams?
        ): ListenableFuture<LibraryResult<MediaItem>> {
            val rootExtras = Bundle().apply {
                putBoolean(MediaConstants.BROWSER_SERVICE_EXTRAS_KEY_SEARCH_SUPPORTED, true)
                putBoolean(MediaConstants.DESCRIPTION_EXTRAS_KEY_CONTENT_STYLE_BROWSABLE, true)
            }
            val libraryParams = MediaLibraryService.LibraryParams.Builder()
                .setExtras(rootExtras).build()
            if ("com.google.android.googlequicksearchbox" == browser.packageName) {
                return Futures.immediateFuture(LibraryResult.ofItem(
                    MediaBrowseTree.browsableItem("__continue__", "Continue Listening", null),
                    libraryParams))
            }
            return Futures.immediateFuture(LibraryResult.ofItem(
                MediaBrowseTree.browsableItem("__root__", "nostr.blue", null),
                libraryParams))
        }

        override fun onGetChildren(
            session: MediaLibraryService.MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            parentId: String, page: Int, pageSize: Int,
            params: MediaLibraryService.LibraryParams?
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> {
            val safePageSize = minOf(pageSize.coerceAtLeast(1), 100)
            val ctx = appContext
            val children: List<MediaItem> = if (ctx != null) {
                when (parentId) {
                    "__root__" -> MediaBrowseTree.getRootChildren()
                    "__continue__" -> MediaBrowseTree.getContinueListening(ctx)
                    "__queue__" -> MediaBrowseTree.getQueue(ctx)
                    "podcasts" -> MediaBrowseTree.getSubscriptions(ctx)
                    "playlists" -> MediaBrowseTree.getPlaylists(ctx)
                    "trending" -> MediaBrowseTree.getTrendingCategories()
                    "trending_podcasts" -> MediaBrowseTree.getTrendingPodcasts(ctx)
                    "trending_music" -> MediaBrowseTree.getTrendingMusic(ctx)
                    else -> {
                        when {
                            parentId.startsWith("podcast:") ->
                                MediaBrowseTree.getPodcastEpisodes(ctx, parentId)
                            parentId.startsWith("playlist:") ->
                                MediaBrowseTree.getPlaylistTracks(ctx, parentId)
                            else -> emptyList()
                        }
                    }
                }
            } else {
                emptyList()
            }
            val paged = children.drop(page * safePageSize).take(safePageSize)
            return Futures.immediateFuture(LibraryResult.ofItemList(paged, params))
        }

        override fun onGetItem(
            session: MediaLibraryService.MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            mediaId: String
        ): ListenableFuture<LibraryResult<MediaItem>> {
            val ctx = appContext ?: return Futures.immediateFuture(
                LibraryResult.ofError(SessionError.ERROR_BAD_VALUE))
            val item = BrowseCache.getItem(ctx, mediaId)
            return if (item != null) {
                Futures.immediateFuture(LibraryResult.ofItem(item, null))
            } else {
                Futures.immediateFuture(LibraryResult.ofError(SessionError.ERROR_BAD_VALUE))
            }
        }

        override fun onAddMediaItems(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>
        ): ListenableFuture<MutableList<MediaItem>> {
            return Futures.immediateFuture(mediaItems)
        }

        @Suppress("UnstableApiUsage")
        override fun onSetMediaItems(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>,
            startIndex: Int,
            startPositionMs: Long
        ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> {
            val index = if (startIndex == C.INDEX_UNSET) 0 else startIndex
            return Futures.immediateFuture(
                MediaSession.MediaItemsWithStartPosition(mediaItems, index, startPositionMs))
        }

        @Suppress("UnstableApiUsage")
        override fun onPlaybackResumption(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
            isForPlayback: Boolean
        ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> {
            val context = appContext ?: return Futures.immediateFuture(
                MediaSession.MediaItemsWithStartPosition(emptyList(), 0, 0))
            val lastItem = BrowseCache.getContinueListeningItems(context).firstOrNull()
            return if (lastItem != null) {
                val position = BrowseCache.getLastPosition(context, lastItem.mediaId)
                Futures.immediateFuture(
                    MediaSession.MediaItemsWithStartPosition(listOf(lastItem), 0, position))
            } else {
                Futures.immediateFuture(
                    MediaSession.MediaItemsWithStartPosition(emptyList(), 0, 0))
            }
        }

        override fun onSearch(
            session: MediaLibraryService.MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            query: String,
            params: MediaLibraryService.LibraryParams?
        ): ListenableFuture<LibraryResult<Void>> {
            val context = appContext ?: return Futures.immediateFuture(LibraryResult.ofVoid())
            CoroutineScope(Dispatchers.IO).launch {
                val results = mutableListOf<MediaItem>()
                results.addAll(BrowseCache.searchCached(context, query))
                try {
                    results.addAll(WavlakeClient.search(context, query))
                } catch (_: Exception) {}
                val deduped = results.distinctBy { it.mediaMetadata.title?.toString()?.lowercase() }
                BrowseCache.saveSearchResults(context, query, deduped)
                session.notifySearchResultChanged(browser, query, deduped.size, params)
            }
            return Futures.immediateFuture(LibraryResult.ofVoid())
        }

        override fun onGetSearchResult(
            session: MediaLibraryService.MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            query: String, page: Int, pageSize: Int,
            params: MediaLibraryService.LibraryParams?
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> {
            val context = appContext ?: return Futures.immediateFuture(
                LibraryResult.ofItemList(ImmutableList.of(), params))
            val safePageSize = pageSize.coerceAtLeast(1)
            val results = BrowseCache.getSearchResults(context, query)
                .drop(page * safePageSize).take(safePageSize)
            return Futures.immediateFuture(LibraryResult.ofItemList(results, params))
        }
    }

    @Synchronized
    private fun prepareCurrent(playWhenReady: Boolean) {
        if (queue.isEmpty()) return
        this.playWhenReady = playWhenReady
        lastError.set(null)
        val player = ensurePlayer()

        val mediaItems = queue.map { item ->
            MediaItem.Builder()
                .setUri(Uri.parse(item.mediaUrl))
                .setMediaMetadata(
                    MediaMetadata.Builder()
                        .setTitle(item.title)
                        .setArtist(item.artist)
                        .setAlbumTitle(item.album)
                        .setArtworkUri(item.albumArtUrl?.let { Uri.parse(it) })
                        .build()
                )
                .build()
        }

        player.setMediaItems(mediaItems, currentIndex, 0L)
        player.playWhenReady = playWhenReady
        player.prepare()
        val item = queue[currentIndex]
        Log.d(AUDIO_TAG, "prepareCurrent: ${item.mediaUrl} isLive=${item.isLiveStream} queueSize=${queue.size} index=$currentIndex")
    }

    @Synchronized
    private fun releasePlayer() {
        player?.removeListener(playerListener)
        player?.release()
        player = null
        mediaLibrarySession?.release()
        mediaLibrarySession = null
    }

    private fun resetSnapshotState() {
        currentIndex = 0
        playWhenReady = false
        lastError.set(null)
    }

    private fun parseQueue(queueJson: String, startIndex: Int): ParsedQueueResult {
        val arr = JSONArray(queueJson)
        var adjustedStartIndex = startIndex
        val items = buildList(arr.length()) {
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                val mediaUrl = obj.optString("media_url")
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
