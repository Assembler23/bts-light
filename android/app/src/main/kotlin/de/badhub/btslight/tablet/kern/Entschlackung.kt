package de.badhub.btslight.tablet.kern

/**
 * Amazon-Apps, die auf einem Zähl-Tablet nur Akku und Hintergrund kosten.
 *
 * Das Einrichtungsskript (`setup-tablet.ps1`/`.sh`) deaktiviert sie per
 * `pm disable-user`; Pakete, die Fire OS als „protected" auch dagegen
 * schützt (OTA-Dienst, Sonderangebote), kann nur der Gerätebesitzer
 * verstecken — das tut die App beim Start über `setApplicationHidden`, mit
 * derselben Liste. Diese Datei ist die Quelle, die Skripte tragen Kopien.
 *
 * Bewusst NICHT hier: Silk (`cloud9`), Appstore (`venezia`), WebView,
 * Launcher, Kindersicherung, Konto-/Gerätedienste (`dcp`, `imp`, `tcomm`)
 * und die eigene App — die braucht das Tablet, oder Fire OS verweigert
 * ohnehin.
 */
object Entschlackung {
    /** Akku-/Hintergrundfresser. */
    val AKKUFRESSER: List<String> = listOf(
        "com.amazon.dee.app",                 // Alexa
        "com.amazon.dee.alexaonandroidos",    // Alexa-Dienst
        "com.amazon.weather",                 // Wetter
        "com.amazon.photos",                  // Amazon Photos (Sync)
        "com.amazon.avod",                    // Prime Video
        "com.amazon.mp3",                     // Amazon Music
        "com.audible.application.kindle",     // Audible
        "com.amazon.kindle",                  // Kindle
        "com.amazon.kindle.kso",              // Sonderangebote (Sperrbildschirm-Werbung)
        "com.amazon.windowshop",              // Amazon Shopping
        "com.amazon.tahoe",                   // Amazon Kids
        "com.amazon.hedwig",                  // Hilfe
        "com.amazon.imdb.tv.mobile.app",      // Freevee/IMDb TV
        "com.amazon.cloud9.kids",             // Silk Kids
    )

    /** Amazon-Systemupdates: kein Fire-OS-Update mitten im Turnier. */
    val UPDATES: List<String> = listOf(
        "com.amazon.device.software.ota",
        "com.amazon.device.software.ota.override",
        "com.amazon.kindle.otter.oobe.forced.ota",
    )

    val ALLE: List<String> = AKKUFRESSER + UPDATES

    /** Pakete, die nie auf der Liste landen dürfen (Wächter für den Test). */
    val TABU: List<String> = listOf(
        "de.badhub.btslight.tablet",
        "com.amazon.cloud9",
        "com.amazon.venezia",
        "com.amazon.webview.chromium",
        "com.amazon.firelauncher",
        "com.amazon.parentalcontrols",
        "com.amazon.dcp",
        "com.amazon.imp",
        "com.amazon.tcomm",
        "com.android.settings",
    )
}
