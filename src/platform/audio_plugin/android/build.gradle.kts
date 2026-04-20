import org.gradle.api.tasks.bundling.AbstractArchiveTask

plugins {
    id("com.android.library") version "8.4.2"
    kotlin("android") version "1.9.24"
}

android {
    namespace = "com.nostr.blue.audio"
    compileSdk = 35

    defaultConfig {
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }

    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
        }
        getByName("debug") {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.media3:media3-exoplayer:1.9.0")
    implementation("androidx.media3:media3-exoplayer-hls:1.9.0")
    implementation("androidx.media3:media3-session:1.9.0")
}

tasks.withType<AbstractArchiveTask>().configureEach {
    archiveBaseName.set("nostrblue-audio-plugin")
}
