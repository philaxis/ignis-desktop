plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// The "waiting for the server" page is shared with the desktop app.
val sharedUiDir = layout.buildDirectory.dir("generated/sharedUi")
val copySharedUi = tasks.register<Copy>("copySharedUi") {
    from(rootProject.file("../ui/connect.html"))
    into(sharedUiDir)
}
tasks.named("preBuild") { dependsOn(copySharedUi) }

// Release signing comes from the environment, so no key or password lives in
// the repository. Without it, `assembleRelease` produces an unsigned APK.
val keystore: String? = System.getenv("IGNIS_ANDROID_KEYSTORE")

android {
    namespace = "dev.philaxis.ignis"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.philaxis.ignis"
        minSdk = 29
        targetSdk = 35
        versionCode = 2
        versionName = "0.2.1"
    }

    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("IGNIS_ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("IGNIS_ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("IGNIS_ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (keystore != null) signingConfig = signingConfigs.getByName("release")
        }
    }

    sourceSets["main"].assets.srcDir(sharedUiDir)

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.activity:activity-ktx:1.9.3")
    implementation("androidx.core:core-ktx:1.13.0")
    implementation("androidx.webkit:webkit:1.14.0")
    testImplementation("junit:junit:4.13.2")
}
