import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Подпись релиза. Ключ в репозитории не лежит. Где он и с какими паролями,
// берётся из переменных окружения RINGLOFT_KEYSTORE, RINGLOFT_KEYSTORE_PASSWORD,
// RINGLOFT_KEY_ALIAS, RINGLOFT_KEY_PASSWORD (так их передаёт release.yml), а
// без них — из keystore.properties рядом с этим проектом (файл в .gitignore):
//   storeFile=/путь/к/ringloft-release.jks
//   storePassword=…
//   keyAlias=ringloft
//   keyPassword=…        (можно не указывать: тогда тот же, что у хранилища)
// Ни того, ни другого — релиз собирается неподписанным, и это не ошибка: так
// проверяют сборку без ключа.
//
// Пароли в CI идут окружением, а не файлом: Properties понимает в файле
// обратную косую черту как экранирование, и пароль с `\` доезжал бы до
// keytool искажённым; в самом файле `\` удваивают. Читается он как UTF-8:
// по умолчанию Properties читает Latin-1, и путь к ключу с кириллицей
// ломался бы.
val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.exists()) {
        propFile.reader(Charsets.UTF_8).use { load(it) }
    }
}

fun signing(env: String, key: String): String? =
    System.getenv(env)?.takeIf { it.isNotEmpty() } ?: keystoreProperties.getProperty(key)

val releaseStore = signing("RINGLOFT_KEYSTORE", "storeFile")
val hasReleaseKey = releaseStore != null

android {
    compileSdk = 36
    namespace = "io.github.hirikayato.ringloft"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "io.github.hirikayato.ringloft"
        minSdk = 26
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (hasReleaseKey) {
            create("release") {
                storeFile = file(releaseStore!!)
                storePassword = signing("RINGLOFT_KEYSTORE_PASSWORD", "storePassword")
                keyAlias = signing("RINGLOFT_KEY_ALIAS", "keyAlias")
                keyPassword = signing("RINGLOFT_KEY_PASSWORD", "keyPassword") ?: storePassword
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            if (hasReleaseKey) {
                signingConfig = signingConfigs.getByName("release")
            }
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")