package de.badhub.btslight.tablet.kern

/**
 * Amazon-/Fremdpakete, die auf einem Zähl-Tablet nur Akku, Hintergrund und
 * Netz kosten. Grundlage ist die Debloat-Liste von Fire-Tools
 * (github.com/mrhaydendp/Fire-Tools, `Fire-Tools/Debloat.txt`), seit Jahren
 * community-erprobt, ergänzt um Amazons OTA-Bausteine.
 *
 * Das Einrichtungsskript (`setup-tablet.ps1`/`.sh`) deaktiviert jedes
 * installierte Paket per `pm disable-user` **und** hält es per `pm suspend`
 * an — Letzteres greift auch bei den Paketen, die Fire OS 8 als „protected"
 * vor dem Deaktivieren schützt (OTA-Dienst, Sonderangebote). Als
 * Gerätebesitzer versteckt die App dieselbe Liste zusätzlich über
 * `setApplicationHidden`. Diese Datei ist die Quelle, die Skripte tragen
 * Kopien; `scripts/test-entschlackung-liste.mjs` hält sie gleich.
 *
 * Feldtest 08.09.2026 (Fire HD 10, Fire OS 8, kompletter Skriptlauf): 81 der
 * Pakete installiert, 57 deaktivierbar, alle 81 anhaltbar, 0 verweigert;
 * nach Neustart App, WLAN und Lobby in Ordnung, Amazon-Prozesse 26 → 17. Fire OS schaltet `adep` und
 * `storagemanager` von selbst wieder frei; OTA und `kindle.kso` laufen trotz
 * Anhalten als Prozess weiter (angehalten = keine Oberfläche, keine
 * Benachrichtigungen).
 *
 * Bewusst NICHT hier (siehe [TABU]): WebView, Silk, Appstore, Launcher,
 * Kindersicherung (Profile Owner, geschützt), die Identitäts-/Middleware-
 * Dienste `dcp`/`imp`, Einstellungen, die Fire-Tastatur (`redstone` — ohne
 * sie lässt sich die Kiosk-PIN nicht tippen) und die eigene App.
 */
object Entschlackung {
    /** Alexa, Sprache und Sprachassistenz. */
    val ALEXA: List<String> = listOf(
        "amazon.speech.sim",                      // Alexa Speech
        "com.amazon.afe.app",                     // Tap to Alexa
        "com.amazon.alexa.multimodal.gemini",     // Alexa Cards
        "com.amazon.alexa.youtube.app",           // Alexa YouTube Player
        "com.amazon.cardinal",                    // Alexa Video Player
        "com.amazon.comms.kids",                  // Alexa Communication
        "com.amazon.dee.alexaonandroidos",        // Alexa on Android OS
        "com.amazon.dee.app",                     // Alexa
        "com.amazon.smartgenie",                  // Alexa Device Dashboard
        "com.amazon.tablet.voiceassistant",       // Alexa Voice Assistant
    )

    /** Inhalte-Apps: Video, Musik, Bücher, Fotos, Shopping, Kids. */
    val INHALTE: List<String> = listOf(
        "com.amazon.avod",                        // Prime Video
        "com.amazon.bioscope",                    // Amazon VideoStore
        "com.amazon.tv.launcher",                 // Amazon VideoStore
        "com.amazon.tv.ottssocompanionapp",       // Amazon TV Provider SSO
        "com.amazon.imdb.tv.mobile.app",          // Freevee (IMDb)
        "com.amazon.mp3",                         // Amazon Music
        "com.audible.application.kindle",         // Audible
        "com.amazon.kindle",                      // Kindle
        "com.amazon.webapp",                      // Kindle Store
        "com.goodreads.kindle",                   // Goodreads
        "com.amazon.kindle.personal_video",       // My Videos
        "com.amazon.photos",                      // Amazon Photos
        "com.amazon.photos.importer",             // Amazon Photos Importer
        "com.amazon.windowshop",                  // Amazon Shopping
        "com.amazon.iris",                        // News
        "com.amazon.weather",                     // Wetter
        "com.amazon.zico",                        // Documents
        "com.kingsoft.office.amz",                // WPS Office for Amazon
        "com.amazon.tahoe",                       // Amazon Kids+ (Kids Space)
        "com.amazon.cloud9.kids",                 // Kids Web Browser
        "com.amazon.ags.app",                     // Amazon GameCircle
        "com.amazon.geo.client.maps",             // Amazon Maps App
        "com.amazon.geo.mapsv2",                  // Map API v2
        "com.amazon.geo.mapsv3.resources",        // Map API v3 Resources
        "com.amazon.geo.mapsv3.services",         // Map API v3 Services
        "com.here.odnp.service",                  // HERE Positioning
        "com.amazon.hedwig",                      // Hilfe / Fire TV Channels
        "com.amazon.csapp",                       // Help App
        "com.amazon.readynowcore",                // On Deck
        "com.amazon.recess",                      // Amazon Recess
        "com.amazon.wallpaper",                   // Amazon Wallpaper
        "com.amazon.firespotlight",               // Amazon Appstore Spotlight
        "com.amazon.kindle.kso",                  // Sonderangebote (Sperrbildschirm-Werbung)
        "com.amazon.kindle.unifiedSearch",        // Unified Search
        "com.amazon.kor.demo",                    // Retail Demo
        "com.amazon.legalsettings",               // Legal Notices
        "com.amazon.logan",                       // Voice View
        "com.amazon.speakscreen",                 // Speak Selection
    )

    /** Metriken, Werbe-IDs, Sync, Konto-Anbindung, Push, Fernwartung. */
    val HINTERGRUND: List<String> = listOf(
        "amazon.jackson19",                       // Jackson19
        "com.amazon.aca",                         // ACA Application
        "com.amazon.advertisingidsettings",       // Advertising ID
        "com.amazon.hybridadidservice",           // Hybrid AD ID Service
        "com.amazon.alta.h2clientservice",        // H2Application
        "com.amazon.h2settingsfortablet",         // Profile Settings (Family Library)
        "com.amazon.application.compatibility.enforcer", // Application Compatibility Enforcer
        "com.amazon.appverification",             // Amazon App Verification
        "com.amazon.charles",                     // Charles Proxy
        "com.amazon.client.metrics",              // Amazon Client Metrics
        "com.amazon.client.metrics.api",          // Amazon Client Metrics API
        "com.amazon.device.metrics",              // Amazon Device Metrics
        "com.amazon.minerva.client.api",          // Amazon Minerva Client API
        "com.amazon.platform",                    // Amazon Metrics Service Application
        "com.amazon.platform.fdra",               // Factory Data Reset Allowlist Manager
        "com.amazon.wirelessmetrics.service",     // Amazon Wireless Metrics Service
        "com.amazon.cloud9.contentservice",       // Silk Browser Content Service
        "com.amazon.cloud9.systembrowserprovider", // Silk Browser Provider
        "com.amazon.communication.discovery",     // Amazon Communication Discovery
        "com.amazon.connectivitydiag",            // Connectivity Diagnostics
        "com.amazon.dcp.contracts.framework.library", // DCP Contracts Framework
        "com.amazon.dcp.contracts.library",       // DCP Platform Contracts
        "com.amazon.dcpms.fos.service",           // DCPMSFOSService
        "com.amazon.device.messaging",            // Amazon Device Messaging (ADM, Push)
        "com.amazon.device.sale.service",         // Amazon Sale Service
        "com.amazon.device.sync",                 // Amazon Sync Service
        "com.amazon.device.sync.sdk.internal",    // Amazon Sync SDK
        "com.amazon.sync.provider.ipc",           // Sync Provider Executor
        "com.amazon.sync.service",                // Amazon Sync Service
        "com.amazon.diode",                       // Diode
        "com.amazon.dp.contacts",                 // Contact Sync Adapter
        "com.amazon.dp.fbcontacts",               // Facebook Sync Adapter
        "com.amazon.dp.logger",                   // Amazon DP Logger
        "com.amazon.dpcclient",                   // Amazon DCPCClient Application
        "com.amazon.fireos.cirruscloud",          // Cirrus Cloud
        "com.amazon.identity.auth.device.authorization", // Mobile Authentication Platform
        "com.amazon.kindle.rdmdeviceadmin",       // Remote Device Management
        "com.amazon.media.session.monitor",       // Media Session Monitor
        "com.amazon.nimh",                        // Arcus Android Client
        "com.amazon.ods.kindleconnect",           // Mayday Screen Sharing
        "com.amazon.pm",                          // Parental Monitoring Service
        "com.amazon.providers.contentsupport",    // Content Support Manager
        "com.amazon.securitysyncclient",          // Security Sync Client
        "com.amazon.sharingservice.android.client.proxy", // Amazon Sharing Proxy Service
        "com.amazon.shpm",                        // Ship Mode
        "com.amazon.tcomm",                       // Amazon Communication Services (Dauerverbindung)
        "com.amazon.tcomm.client",                // Amazon Communication Services Client Library
        "com.amazon.tcomm.jackson",               // Amazon Communication Service (Jackson19)
        "com.amazon.whisperlink.core.android",    // Whisperplay Daemon
        "com.amazon.whisperplay.contracts",       // Whisperlink SDK
        "com.amazon.whisperplay.service.install", // Whisperlink Installer
        "com.amazon.watson",                      // Watson (Amazon-Dienst, priv-app)
        "com.fireos.arcus.proxy",                 // Arcus Proxy
        "com.fireos.usagestats.proxy",            // Amazon Usage Stats Map Proxy
        "com.android.bookmarkprovider",           // Bookmarks Provider
        "com.android.carrierconfig",              // Carrier Network Configuration
        "com.android.dreams.basic",               // Screensaver
        "com.android.email",                      // AOSP Mail
        "com.android.protips",                    // Home Screen Tips
        "com.android.providers.downloads.ui",     // Downloads App
        "com.android.providers.partnerbookmarks", // Provider Bookmarks
        "com.android.quicksearchbox",             // Search
    )

    /** Amazon-Systemupdates: kein Fire-OS-Update mitten im Turnier. */
    val UPDATES: List<String> = listOf(
        "com.amazon.device.software.ota",
        "com.amazon.device.software.ota.override",
        "com.amazon.kindle.otter.oobe",           // Device Setup
        "com.amazon.kindle.otter.oobe.forced.ota",
    )

    val ALLE: List<String> = ALEXA + INHALTE + HINTERGRUND + UPDATES

    /** Pakete, die nie auf der Liste landen dürfen (Wächter für den Test). */
    val TABU: List<String> = listOf(
        "de.badhub.btslight.tablet",
        "com.amazon.cloud9",                      // Silk (Notausgang für Fehlersuche)
        "com.amazon.venezia",                     // Appstore
        "com.amazon.webview.chromium",            // WebView — die App selbst
        "com.amazon.firelauncher",                // Launcher (protected)
        "com.amazon.parentalcontrols",            // Kindersicherung (Profile Owner, protected)
        "com.amazon.dcp",                         // Device-Middleware
        "com.amazon.imp",                         // Identity Mobile Platform
        "com.amazon.redstone",                    // Fire-Tastatur — sonst keine PIN-Eingabe
        "com.android.settings",
        "com.android.systemui",
        "com.android.shell",                      // adb!
        "com.android.providers.settings",
        "com.android.providers.downloads",        // WebView-Downloads
        "com.android.permissioncontroller",
        "com.amazon.kindle.cms",                  // Launcher-Backend
        "com.amazon.wifilocker",
    )
}
