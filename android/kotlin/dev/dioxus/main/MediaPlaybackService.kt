package dev.dioxus.main

import android.app.Service
import android.content.Intent
import android.os.IBinder

class MediaPlaybackService : Service() {
    override fun onCreate() {
        super.onCreate()
        NativeAudioBridge.attachService(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        NativeAudioBridge.attachService(this)
        when (intent?.action) {
            NativeAudioBridge.ACTION_PLAY -> NativeAudioBridge.play(applicationContext)
            NativeAudioBridge.ACTION_PAUSE -> NativeAudioBridge.pause(applicationContext)
            NativeAudioBridge.ACTION_NEXT -> NativeAudioBridge.skipNext(applicationContext)
            NativeAudioBridge.ACTION_PREVIOUS -> NativeAudioBridge.skipPrevious(applicationContext)
            NativeAudioBridge.ACTION_STOP -> NativeAudioBridge.stop(applicationContext)
        }
        return START_STICKY
    }

    override fun onDestroy() {
        NativeAudioBridge.detachService(this)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
