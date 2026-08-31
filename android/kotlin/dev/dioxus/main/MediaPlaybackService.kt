package dev.dioxus.main

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.util.Log
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaLibraryService
import androidx.media3.session.MediaSession

class MediaPlaybackService : MediaLibraryService() {
    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaLibrarySession? {
        return NativeAudioBridge.mediaLibrarySession
    }

    override fun onCreate() {
        ensureChannel()
        setMediaNotificationProvider(
            DefaultMediaNotificationProvider.Builder(this)
                .setChannelId(CHANNEL_ID)
                .setNotificationId(NOTIFICATION_ID)
                .build()
        )
        super.onCreate()
        NativeAudioBridge.attachService(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        NativeAudioBridge.attachService(this)
        val session = NativeAudioBridge.mediaLibrarySession
        if (session != null && session.player.playbackState != androidx.media3.common.Player.STATE_IDLE) {
            return super.onStartCommand(intent, flags, startId)
        }
        // After a process crash the service may be restarted from the background,
        // where startForeground() throws ForegroundServiceStartNotAllowedException
        // on Android 12+. That exception escaped and crash-looped the process;
        // log instead and let the session (if any) keep playing without the
        // notification until the app is foregrounded again.
        try {
            val notification = Notification.Builder(this, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.ic_media_play)
                .setContentTitle("nostr.blue")
                .setContentText("Preparing media...")
                .setOngoing(true)
                .build()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
            } else {
                startForeground(NOTIFICATION_ID, notification)
            }
        } catch (e: Exception) {
            Log.w(TAG, "startForeground not allowed (background restart); skipping notification", e)
        }
        return super.onStartCommand(intent, flags, startId)
    }

    override fun onDestroy() {
        NativeAudioBridge.detachService(this)
        super.onDestroy()
    }

    private fun ensureChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            if (manager.getNotificationChannel(CHANNEL_ID) == null) {
                manager.createNotificationChannel(
                    NotificationChannel(CHANNEL_ID, "Media playback", NotificationManager.IMPORTANCE_LOW)
                )
            }
        }
    }

    companion object {
        private const val TAG = "MediaPlaybackService"
        private const val CHANNEL_ID = "nostrblue_media"
        private const val NOTIFICATION_ID = 7001
    }
}
