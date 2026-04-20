package com.nostr.blue.audio

import android.app.Activity

class AudioPlugin(activity: Activity) {
    private val context = activity.applicationContext

    fun setQueue(queueJson: String, startIndex: Int, playWhenReady: Boolean): String =
        NativeAudioBridge.setQueue(context, queueJson, startIndex, playWhenReady)

    fun play(): String = NativeAudioBridge.play(context)

    fun pause(): String = NativeAudioBridge.pause(context)

    fun skipNext(): String = NativeAudioBridge.skipNext(context)

    fun skipPrevious(): String = NativeAudioBridge.skipPrevious(context)

    fun seekTo(positionMs: Long): String = NativeAudioBridge.seekTo(context, positionMs)

    fun setPlaybackSpeed(speed: Float): String = NativeAudioBridge.setPlaybackSpeed(context, speed)

    fun setVolume(volume: Float): String = NativeAudioBridge.setVolume(context, volume)

    fun stop(): String = NativeAudioBridge.stop(context)

    fun clearQueue(): String = NativeAudioBridge.clearQueue(context)

    fun getSnapshot(): String = NativeAudioBridge.getSnapshot(context)
}
