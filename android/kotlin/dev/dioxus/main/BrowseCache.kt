package dev.dioxus.main

import android.content.Context
import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import org.json.JSONArray
import org.json.JSONObject

object BrowseCache {
    private const val PREFS_NAME = "nostr_blue_browse"
    private const val KEY_CONTINUE_LISTENING = "continue_listening"
    private const val KEY_QUEUE = "queue"
    private const val KEY_SUBSCRIPTIONS = "subscriptions"
    private const val KEY_TRENDING_PODCASTS = "trending_podcasts"
    private const val KEY_TRENDING_MUSIC = "trending_music"
    private const val KEY_EPISODES_PREFIX = "episodes:"
    private const val KEY_PLAYLISTS = "playlists"
    private const val KEY_PLAYLIST_PREFIX = "playlist:"
    private const val KEY_SEARCH_PREFIX = "search:"
    private const val KEY_ITEM_PREFIX = "item:"
    private const val KEY_POSITION_PREFIX = "position:"

    private fun prefs(ctx: Context) =
        ctx.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    fun save(ctx: Context, key: String, json: String) {
        prefs(ctx).edit().putString(key, json).apply()
    }

    fun savePosition(ctx: Context, mediaId: String, positionMs: Long) {
        prefs(ctx).edit().putLong(KEY_POSITION_PREFIX + mediaId, positionMs).apply()
    }

    fun getItem(ctx: Context, mediaId: String): MediaItem? {
        val json = prefs(ctx).getString(KEY_ITEM_PREFIX + mediaId, null) ?: return null
        return parseMusicTrackItem(json)
    }

    fun getLastPosition(ctx: Context, mediaId: String): Long {
        return prefs(ctx).getLong(KEY_POSITION_PREFIX + mediaId, 0L)
    }

    fun getContinueListeningItems(ctx: Context): List<MediaItem> {
        return readTrackList(ctx, KEY_CONTINUE_LISTENING)
    }

    fun getQueueItems(ctx: Context): List<MediaItem> {
        return readTrackList(ctx, KEY_QUEUE)
    }

    fun getSubscriptionsList(ctx: Context): List<JSONObject> {
        return readJsonArray(ctx, KEY_SUBSCRIPTIONS)
    }

    fun getEpisodes(ctx: Context, key: String): List<MediaItem> {
        return readTrackList(ctx, KEY_EPISODES_PREFIX + key)
    }

    fun getTrendingPodcastsList(ctx: Context): List<JSONObject> {
        return readJsonArray(ctx, KEY_TRENDING_PODCASTS)
    }

    fun getTrendingMusicList(ctx: Context): List<MediaItem> {
        return readTrackList(ctx, KEY_TRENDING_MUSIC)
    }

    fun getPlaylistsList(ctx: Context): List<JSONObject> {
        return readJsonArray(ctx, KEY_PLAYLISTS)
    }

    fun getPlaylistTrackList(ctx: Context, naddr: String): List<MediaItem> {
        return readTrackList(ctx, KEY_PLAYLIST_PREFIX + naddr)
    }

    fun searchCached(ctx: Context, query: String): List<MediaItem> {
        val q = query.lowercase()
        val results = mutableListOf<MediaItem>()
        for (track in getContinueListeningItems(ctx)) {
            if (track.mediaMetadata.title?.toString()?.lowercase()?.contains(q) == true) {
                results.add(track)
            }
        }
        for (track in getQueueItems(ctx)) {
            if (track.mediaMetadata.title?.toString()?.lowercase()?.contains(q) == true) {
                results.add(track)
            }
        }
        return results
    }

    fun saveSearchResults(ctx: Context, query: String, items: List<MediaItem>) {
        val arr = JSONArray()
        for (item in items) {
            val json = prefs(ctx).getString(KEY_ITEM_PREFIX + item.mediaId, null)
            if (json != null) arr.put(JSONObject(json))
        }
        prefs(ctx).edit().putString(KEY_SEARCH_PREFIX + query.lowercase(), arr.toString()).apply()
    }

    fun getSearchResults(ctx: Context, query: String): List<MediaItem> {
        return readTrackList(ctx, KEY_SEARCH_PREFIX + query.lowercase())
    }

    private fun readTrackList(ctx: Context, key: String): List<MediaItem> {
        val json = prefs(ctx).getString(key, "[]") ?: "[]"
        val arr = JSONArray(json)
        return (0 until arr.length()).mapNotNull { i ->
            parseMusicTrackItem(arr.getJSONObject(i).toString())
        }
    }

    private fun readJsonArray(ctx: Context, key: String): List<JSONObject> {
        val json = prefs(ctx).getString(key, "[]") ?: "[]"
        val arr = JSONArray(json)
        return (0 until arr.length()).mapNotNull { i ->
            try { arr.getJSONObject(i) } catch (_: Exception) { null }
        }
    }

    private fun parseMusicTrackItem(json: String): MediaItem? {
        return try {
            val obj = JSONObject(json)
            val mediaId = obj.optString("id").takeIf { it.isNotBlank() } ?: return null
            val metadata = MediaMetadata.Builder()
                .setTitle(obj.optString("title"))
                .setArtist(obj.optString("artist"))
                .setAlbumTitle(obj.optString("album").takeIf { it.isNotBlank() && it != "null" })
                .setIsPlayable(true)
                .setIsBrowsable(false)
            obj.optString("album_art_url").takeIf { it.isNotBlank() && it != "null" }?.let {
                metadata.setArtworkUri(Uri.parse(it))
            }
            val mediaUrl = obj.optString("media_url").takeIf { it.isNotBlank() }
            MediaItem.Builder()
                .setMediaId(mediaId)
                .setUri(mediaUrl?.let { Uri.parse(it) })
                .setMediaMetadata(metadata.build())
                .build()
        } catch (_: Exception) { null }
    }
}
