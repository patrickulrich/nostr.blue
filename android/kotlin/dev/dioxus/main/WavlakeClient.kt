package dev.dioxus.main

import android.content.Context
import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import java.net.URLEncoder
import java.util.concurrent.TimeUnit

object WavlakeClient {
    private const val BASE = "https://wavlake.com/api/v1"
    private val client = OkHttpClient.Builder()
        .connectTimeout(5, TimeUnit.SECONDS)
        .readTimeout(8, TimeUnit.SECONDS)
        .build()

    fun search(ctx: Context, term: String, max: Int = 21): List<MediaItem> {
        val url = "$BASE/content/search?term=${URLEncoder.encode(term, "UTF-8")}"
        val request = Request.Builder().url(url).build()
        return try {
            val response = client.newCall(request).execute()
            if (!response.isSuccessful) return emptyList()
            val body = response.body?.string() ?: return emptyList()
            val arr = JSONArray(body)
            val results = mutableListOf<MediaItem>()
            for (i in 0 until arr.length()) {
                if (results.size >= max) break
                val obj = arr.getJSONObject(i)
                val type = obj.optString("type", "")
                if (type != "track") continue
                val item = resolveTrackItem(obj)
                if (item != null) {
                    results.add(item)
                }
            }
            results
        } catch (_: Exception) { emptyList() }
    }

    private fun resolveTrackItem(searchResult: org.json.JSONObject): MediaItem? {
        val id = searchResult.optString("id") ?: return null
        val title = searchResult.optString("title", searchResult.optString("name"))
        val artist = searchResult.optString("artist")
        val art = searchResult.optString("albumArtUrl")
            ?: searchResult.optString("artistArtUrl")
        val duration = searchResult.optInt("duration", 0)
        val trackUrl = "$BASE/content/track/$id"
        val request = Request.Builder().url(trackUrl).build()
        return try {
            val response = client.newCall(request).execute()
            if (!response.isSuccessful) return null
            val body = response.body?.string() ?: return null
            val trackArr = JSONArray(body)
            if (trackArr.length() == 0) return null
            val trackObj = trackArr.getJSONObject(0)
            val mediaUrl = trackObj.optString("mediaUrl") ?: return null
            val metadata = MediaMetadata.Builder()
                .setTitle(title)
                .setArtist(artist)
                .setIsPlayable(true)
                .setIsBrowsable(false)
            art?.takeIf { it.isNotBlank() }?.let { metadata.setArtworkUri(Uri.parse(it)) }
            if (duration > 0) metadata.setDurationMs(duration.toLong() * 1000)
            MediaItem.Builder()
                .setMediaId("track:wavlake:$id")
                .setUri(Uri.parse(mediaUrl))
                .setMediaMetadata(metadata.build())
                .build()
        } catch (_: Exception) { null }
    }
}
