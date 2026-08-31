package dev.dioxus.main

import android.content.Context
import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata

object MediaBrowseTree {
    fun getRootChildren(): List<MediaItem> = listOf(
        browsableItem("__continue__", "Continue Listening", null),
        browsableItem("__queue__", "Queue", null),
        browsableItem("podcasts", "Podcasts", null),
        browsableItem("playlists", "Playlists", null),
        browsableItem("downloads", "Downloads", null),
        browsableItem("trending", "Trending", null)
    )

    fun getDownloads(ctx: Context): List<MediaItem> =
        BrowseCache.getDownloadsList(ctx)

    fun getContinueListening(ctx: Context): List<MediaItem> =
        BrowseCache.getContinueListeningItems(ctx).take(8)

    fun getQueue(ctx: Context): List<MediaItem> =
        BrowseCache.getQueueItems(ctx)

    fun getSubscriptions(ctx: Context): List<MediaItem> {
        return BrowseCache.getSubscriptionsList(ctx).mapNotNull { sub ->
            try {
                val title = sub.optString("title", "Unknown")
                val image = sub.optString("image")
                val guid = sub.optString("podcast_guid")
                val coord = sub.optString("nostr_coordinate")
                val id = if (guid.isNotBlank()) "podcast:rss:$guid"
                         else if (coord.isNotBlank()) "podcast:nostr:$coord"
                         else return@mapNotNull null
                browsableItem(id, title, image.takeIf { it.isNotBlank() })
            } catch (_: Exception) { null }
        }
    }

    fun getPlaylists(ctx: Context): List<MediaItem> {
        return BrowseCache.getPlaylistsList(ctx).mapNotNull { pl ->
            try {
                val naddr = pl.optString("naddr")
                val title = pl.optString("title", "Unknown Playlist")
                if (naddr.isBlank()) return@mapNotNull null
                browsableItem("playlist:$naddr", title, null)
            } catch (_: Exception) { null }
        }
    }

    fun getTrendingCategories(): List<MediaItem> = listOf(
        browsableItem("trending_podcasts", "Trending Podcasts", null),
        browsableItem("trending_music", "Trending Music", null)
    )

    fun getTrendingPodcasts(ctx: Context): List<MediaItem> {
        return BrowseCache.getTrendingPodcastsList(ctx).mapNotNull { feed ->
            try {
                val id = "podcast:rss:${feed.optLong("id")}"
                val title = feed.optString("title", "Unknown")
                val art = feed.optString("artwork").takeIf { it.isNotBlank() }
                    ?: feed.optString("image").takeIf { it.isNotBlank() }
                browsableItem(id, title, art)
            } catch (_: Exception) { null }
        }
    }

    fun getTrendingMusic(ctx: Context): List<MediaItem> =
        BrowseCache.getTrendingMusicList(ctx)

    fun getPodcastEpisodes(ctx: Context, parentId: String): List<MediaItem> {
        val key = parentId.removePrefix("podcast:")
        val episodes = BrowseCache.getEpisodes(ctx, key)
        if (episodes.isEmpty()) {
            return listOf(placeholderItem("Open nostr.blue to load episodes"))
        }
        return episodes
    }

    fun getPlaylistTracks(ctx: Context, parentId: String): List<MediaItem> =
        BrowseCache.getPlaylistTrackList(ctx, parentId.removePrefix("playlist:"))

    fun browsableItem(id: String, title: String, artUrl: String?): MediaItem {
        val metadata = MediaMetadata.Builder()
            .setTitle(title)
            .setIsBrowsable(true)
            .setIsPlayable(false)
        artUrl?.takeIf { it.isNotBlank() }?.let { metadata.setArtworkUri(Uri.parse(it)) }
        return MediaItem.Builder()
            .setMediaId(id)
            .setMediaMetadata(metadata.build())
            .build()
    }

    private fun placeholderItem(message: String): MediaItem {
        val metadata = MediaMetadata.Builder()
            .setTitle(message)
            .setIsPlayable(false)
            .setIsBrowsable(false)
            .build()
        return MediaItem.Builder()
            .setMediaId("placeholder")
            .setMediaMetadata(metadata)
            .build()
    }
}
