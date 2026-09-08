# Einmalige Einrichtung eines Fire-/Android-Tablets als bts-light-Zähltablet.
# Voraussetzung: Tablet zurückgesetzt, Amazon-/Google-Anmeldung übersprungen,
# WLAN verbunden (oder -Wlan/-WlanPasswort angeben), ADB-Debugging an, USB angeschlossen, adb im PATH.
param(
  [string]$Apk = "bts-light-tablet.apk",
  # Hallen-WLAN gleich mit einrichten (WPA2; leer = WLAN von Hand verbinden).
  [string]$Wlan = "",
  [string]$WlanPasswort = "",
  # Amazon-Apps behalten (Alexa, Video, Kindle … und den OTA-Dienst).
  [switch]$OhneEntschlacken
)

# "Continue", nicht "Stop": Windows PowerShell 5.1 verpackt stderr-Zeilen
# nativer Programme (adb, dpm, am) in ErrorRecords, sobald der Host oder ein
# Aufrufer stderr umleitet — unter "Stop" stürbe das Skript dann an jeder
# Warnung von Android. Was fatal ist (Gerät, Install), prüft das Skript
# selbst über Exit-Codes.
$ErrorActionPreference = "Continue"
$Paket = "de.badhub.btslight.tablet"

# WLAN per adb verbinden (`cmd wifi connect-network`, ab Android 11) und auf
# eine IPv4-Adresse warten. Scheitert nur weich: Ohne WLAN geht die
# Einrichtung weiter, die App sucht den Turnier-PC dann später.
function WlanEinrichten([string]$Ssid, [string]$Passwort) {
  adb shell cmd wifi set-wifi-enabled enabled | Out-Null
  $r = (adb shell cmd wifi connect-network "$Ssid" wpa2 "$Passwort" 2>&1 | Out-String)
  if ($r -notmatch "Connection initiated") { Write-Warning "WLAN-Befehl nicht angenommen: $($r.Trim()) – WLAN bitte von Hand verbinden."; return }
  for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Seconds 1
    $ip = (adb shell ip -4 addr show wlan0 2>$null | Select-String "inet (\d+\.\d+\.\d+\.\d+)").Matches
    if ($ip.Count -gt 0) { Write-Host "  WLAN $Ssid verbunden, IP $($ip[0].Groups[1].Value)"; return }
  }
  Write-Warning "WLAN ${Ssid}: nach 30 s keine IP-Adresse – SSID/Passwort prüfen, Einrichtung läuft weiter."
}

# Amazon-/Fremdpakete, die auf einem Zähl-Tablet nur Akku, Hintergrund und
# Netz kosten — Grundlage: Debloat-Liste von Fire-Tools. Dieselbe Liste wie
# in setup-tablet.sh und kern/Entschlackung.kt (dort pflegen, die Skripte
# erzeugt/prüft scripts/test-entschlackung-liste.mjs). Bewusst NICHT dabei:
# WebView, Silk, Appstore, Launcher, Kindersicherung, dcp/imp, Fire-Tastatur
# (redstone — sonst keine PIN-Eingabe), Einstellungen.
$AmazonApps = @(
  "amazon.speech.sim",                           # Alexa Speech
  "com.amazon.afe.app",                          # Tap to Alexa
  "com.amazon.alexa.multimodal.gemini",          # Alexa Cards
  "com.amazon.alexa.youtube.app",                # Alexa YouTube Player
  "com.amazon.cardinal",                         # Alexa Video Player
  "com.amazon.comms.kids",                       # Alexa Communication
  "com.amazon.dee.alexaonandroidos",             # Alexa on Android OS
  "com.amazon.dee.app",                          # Alexa
  "com.amazon.smartgenie",                       # Alexa Device Dashboard
  "com.amazon.tablet.voiceassistant",            # Alexa Voice Assistant
  "com.amazon.avod",                             # Prime Video
  "com.amazon.bioscope",                         # Amazon VideoStore
  "com.amazon.tv.launcher",                      # Amazon VideoStore
  "com.amazon.tv.ottssocompanionapp",            # Amazon TV Provider SSO
  "com.amazon.imdb.tv.mobile.app",               # Freevee (IMDb)
  "com.amazon.mp3",                              # Amazon Music
  "com.audible.application.kindle",              # Audible
  "com.amazon.kindle",                           # Kindle
  "com.amazon.webapp",                           # Kindle Store
  "com.goodreads.kindle",                        # Goodreads
  "com.amazon.kindle.personal_video",            # My Videos
  "com.amazon.photos",                           # Amazon Photos
  "com.amazon.photos.importer",                  # Amazon Photos Importer
  "com.amazon.windowshop",                       # Amazon Shopping
  "com.amazon.iris",                             # News
  "com.amazon.weather",                          # Wetter
  "com.amazon.zico",                             # Documents
  "com.kingsoft.office.amz",                     # WPS Office for Amazon
  "com.amazon.tahoe",                            # Amazon Kids+ (Kids Space)
  "com.amazon.cloud9.kids",                      # Kids Web Browser
  "com.amazon.ags.app",                          # Amazon GameCircle
  "com.amazon.geo.client.maps",                  # Amazon Maps App
  "com.amazon.geo.mapsv2",                       # Map API v2
  "com.amazon.geo.mapsv3.resources",             # Map API v3 Resources
  "com.amazon.geo.mapsv3.services",              # Map API v3 Services
  "com.here.odnp.service",                       # HERE Positioning
  "com.amazon.hedwig",                           # Hilfe / Fire TV Channels
  "com.amazon.csapp",                            # Help App
  "com.amazon.readynowcore",                     # On Deck
  "com.amazon.recess",                           # Amazon Recess
  "com.amazon.wallpaper",                        # Amazon Wallpaper
  "com.amazon.firespotlight",                    # Amazon Appstore Spotlight
  "com.amazon.kindle.kso",                       # Sonderangebote (Sperrbildschirm-Werbung)
  "com.amazon.kindle.unifiedSearch",             # Unified Search
  "com.amazon.kor.demo",                         # Retail Demo
  "com.amazon.legalsettings",                    # Legal Notices
  "com.amazon.logan",                            # Voice View
  "com.amazon.speakscreen",                      # Speak Selection
  "amazon.jackson19",                            # Jackson19
  "com.amazon.aca",                              # ACA Application
  "com.amazon.advertisingidsettings",            # Advertising ID
  "com.amazon.hybridadidservice",                # Hybrid AD ID Service
  "com.amazon.alta.h2clientservice",             # H2Application
  "com.amazon.h2settingsfortablet",              # Profile Settings (Family Library)
  "com.amazon.application.compatibility.enforcer", # Application Compatibility Enforcer
  "com.amazon.appverification",                  # Amazon App Verification
  "com.amazon.charles",                          # Charles Proxy
  "com.amazon.client.metrics",                   # Amazon Client Metrics
  "com.amazon.client.metrics.api",               # Amazon Client Metrics API
  "com.amazon.device.metrics",                   # Amazon Device Metrics
  "com.amazon.minerva.client.api",               # Amazon Minerva Client API
  "com.amazon.platform",                         # Amazon Metrics Service Application
  "com.amazon.platform.fdra",                    # Factory Data Reset Allowlist Manager
  "com.amazon.wirelessmetrics.service",          # Amazon Wireless Metrics Service
  "com.amazon.cloud9.contentservice",            # Silk Browser Content Service
  "com.amazon.cloud9.systembrowserprovider",     # Silk Browser Provider
  "com.amazon.communication.discovery",          # Amazon Communication Discovery
  "com.amazon.connectivitydiag",                 # Connectivity Diagnostics
  "com.amazon.dcp.contracts.framework.library",  # DCP Contracts Framework
  "com.amazon.dcp.contracts.library",            # DCP Platform Contracts
  "com.amazon.dcpms.fos.service",                # DCPMSFOSService
  "com.amazon.device.messaging",                 # Amazon Device Messaging (ADM, Push)
  "com.amazon.device.sale.service",              # Amazon Sale Service
  "com.amazon.device.sync",                      # Amazon Sync Service
  "com.amazon.device.sync.sdk.internal",         # Amazon Sync SDK
  "com.amazon.sync.provider.ipc",                # Sync Provider Executor
  "com.amazon.sync.service",                     # Amazon Sync Service
  "com.amazon.diode",                            # Diode
  "com.amazon.dp.contacts",                      # Contact Sync Adapter
  "com.amazon.dp.fbcontacts",                    # Facebook Sync Adapter
  "com.amazon.dp.logger",                        # Amazon DP Logger
  "com.amazon.dpcclient",                        # Amazon DCPCClient Application
  "com.amazon.fireos.cirruscloud",               # Cirrus Cloud
  "com.amazon.identity.auth.device.authorization", # Mobile Authentication Platform
  "com.amazon.kindle.rdmdeviceadmin",            # Remote Device Management
  "com.amazon.media.session.monitor",            # Media Session Monitor
  "com.amazon.nimh",                             # Arcus Android Client
  "com.amazon.ods.kindleconnect",                # Mayday Screen Sharing
  "com.amazon.pm",                               # Parental Monitoring Service
  "com.amazon.providers.contentsupport",         # Content Support Manager
  "com.amazon.securitysyncclient",               # Security Sync Client
  "com.amazon.sharingservice.android.client.proxy", # Amazon Sharing Proxy Service
  "com.amazon.shpm",                             # Ship Mode
  "com.amazon.tcomm",                            # Amazon Communication Services (Dauerverbindung)
  "com.amazon.tcomm.client",                     # Amazon Communication Services Client Library
  "com.amazon.tcomm.jackson",                    # Amazon Communication Service (Jackson19)
  "com.amazon.whisperlink.core.android",         # Whisperplay Daemon
  "com.amazon.whisperplay.contracts",            # Whisperlink SDK
  "com.amazon.whisperplay.service.install",      # Whisperlink Installer
  "com.amazon.watson",                           # Watson (Amazon-Dienst, priv-app)
  "com.fireos.arcus.proxy",                      # Arcus Proxy
  "com.fireos.usagestats.proxy",                 # Amazon Usage Stats Map Proxy
  "com.android.bookmarkprovider",                # Bookmarks Provider
  "com.android.carrierconfig",                   # Carrier Network Configuration
  "com.android.dreams.basic",                    # Screensaver
  "com.android.email",                           # AOSP Mail
  "com.android.protips",                         # Home Screen Tips
  "com.android.providers.downloads.ui",          # Downloads App
  "com.android.providers.partnerbookmarks",      # Provider Bookmarks
  "com.android.quicksearchbox",                  # Search
  "com.amazon.device.software.ota",              # 
  "com.amazon.device.software.ota.override",     # 
  "com.amazon.kindle.otter.oobe",                # Device Setup
  "com.amazon.kindle.otter.oobe.forced.ota"      # 
)

# Deaktiviert und hält die Amazon-Apps für Nutzer 0 an; je Paket eine Zeile,
# Verweigerung bricht nicht ab (stderr-Zeilen sind unter "Continue" harmlos).
# Je installiertem Paket zwei Griffe: `pm disable-user` (schaltet ab, geht
# nicht bei „protected" Paketen) und `pm suspend` (hält an: keine Oberfläche,
# keine Benachrichtigungen — geht auch bei protected). Zurück mit
# `pm unsuspend <paket>` + `pm enable --user 0 <paket>`.
function Entschlacken {
  $installiert = (adb shell pm list packages --user 0) -replace "package:", "" -replace "\r", ""
  $dis = 0; $sus = 0; $nichts = 0; $fehlt = 0
  foreach ($p in $AmazonApps) {
    if ($installiert -notcontains $p) { $fehlt++; continue }
    $ok1 = ((adb shell pm disable-user --user 0 $p 2>&1 | Out-String) -match "new state: disabled")
    $ok2 = ((adb shell pm suspend $p 2>&1 | Out-String) -match "suspended state: true")
    if ($ok1) { $dis++ }; if ($ok2) { $sus++ }
    if ($ok1 -and $ok2) { Write-Host "  aus+angehalten $p" }
    elseif ($ok2) { Write-Host "  angehalten     $p (deaktivieren verweigert: protected)" }
    elseif ($ok1) { Write-Host "  aus            $p (anhalten verweigert)" }
    else { Write-Host "  verweigert     $p"; $nichts++ }
  }
  Write-Host "Entschlackt: $dis deaktiviert, $sus angehalten, $nichts verweigert, $fehlt nicht vorhanden."
}

Write-Host "Geräte:"; adb devices
$geraete = @(adb devices | Select-String "device$").Count
if ($geraete -ne 1) { Write-Error "Genau ein Tablet per USB anschließen (gefunden: $geraete)."; exit 1 }

if ($Wlan) { Write-Host "WLAN einrichten: $Wlan"; WlanEinrichten $Wlan $WlanPasswort }

# Device Owner geht nur ohne eingerichtete Konten. Nur Warnung: Fire OS 8 hat
# auch ohne Anmeldung drei interne Konten (Typ `amazon.account`), und der
# Besitzer-Schritt ist unten ohnehin nicht mehr fatal.
$konten = adb shell dumpsys account | Select-String "Account \{"
if ($konten) {
  Write-Warning "Konten auf dem Tablet gefunden ($($konten.Count)) – der Gerätebesitzer-Schritt wird damit scheitern (auf Fire OS 8 normal). Bei einem angemeldeten Amazon-Konto: abmelden."
}

# Eine noch fixierte alte Fassung blockiert den Start der neuen („Lock Task
# Mode violation", Fehlercode 101) — vorher lösen, harmlos wenn nicht fixiert.
adb shell am task lock stop | Out-Null
Write-Host "APK installieren: $Apk"
# $ErrorActionPreference greift bei nativen Programmen (adb) nicht – Exit-Code selbst prüfen.
adb install -r $Apk
if ($LASTEXITCODE -ne 0) { Write-Error "adb install fehlgeschlagen (Exit $LASTEXITCODE)."; exit 1 }

Write-Host "Gerätebesitzer setzen"
adb shell dpm set-device-owner "$Paket/.kiosk.KioskAdminReceiver" 2>&1 | Out-String | Write-Host
if ($LASTEXITCODE -ne 0) {
  # Auf Fire OS 8 scheitert das IMMER (Kindersicherung ist Profile Owner, auch
  # nach Werksreset) — dann läuft die Einrichtung ohne Besitzer weiter; die
  # Sperre bleibt das weiche Anheften.
  Write-Warning "Gerätebesitzer konnte nicht gesetzt werden (Exit $LASTEXITCODE). Auf Fire OS 8 normal (Kindersicherung ist Profile Owner); sonst: noch ein Konto, oder schon ein anderer Besitzer. Weiter ohne Besitzer."
}

# Ohne Besitzer erledigt die App das nicht selbst: Bildschirm am Ladekabel
# an, Sperrbildschirm (mit Werbung) aus, und der Fire-OS-Schalter
# „Touch-Funktion deaktivieren" der Kindersicherung aus — sonst schluckt der
# Toddler Mode beim Anheften jeden Touch (Feldtest 08.09.2026).
Write-Host "Bildschirm: Wachhalten am Ladekabel, Sperrbildschirm aus, Touch beim Fixieren erlauben"
adb shell settings put global stay_on_while_plugged_in 7
adb shell locksettings set-disabled true
adb shell settings put secure toddler_mode_default_value 0
# Autostart ohne Gerätebesitzer: Android 11 bricht Activity-Starts aus
# BOOT_COMPLETED ab („Abort background activity starts"); dieser
# Konfigurationswert hebt das gerätweit auf (Feldtest 08.09.2026: überlebt
# Neustarts, SYSTEM_ALERT_WINDOW und der alte Entwickler-Schalter halfen
# auf Fire OS nicht). Auf einem Zähl-Tablet unbedenklich.
Write-Host "Autostart: Hintergrund-Aktivitätsstarts erlauben"
adb shell device_config put activity_manager default_background_activity_starts_enabled true
# Ohne Besitzer zeigt Android bei jedem Fixieren den Dialog „App ist auf dem
# Bildschirm fixiert" — der Bedienungshilfe-Dienst der App tippt „Verstanden"
# selbst. Bestehende Dienste bleiben erhalten.
Write-Host "Fixieren: Bestätigungsdienst einschalten"
$dienst = "$Paket/.kiosk.BestaetigungsDienst"
$alt = (adb shell settings get secure enabled_accessibility_services | Out-String).Trim()
if ($alt -eq "null" -or [string]::IsNullOrWhiteSpace($alt)) { $neu = $dienst } elseif ($alt -like "*$dienst*") { $neu = $alt } else { $neu = "${alt}:$dienst" }
adb shell settings put secure enabled_accessibility_services "$neu"
adb shell settings put secure accessibility_enabled 1
# Akku: Energiesparmodus. Fire OS setzt `low_power` beim Boot am Ladekabel
# zurück — verlässlich ist die Automatik-Schwelle: bei 100 % greift der
# Sparmodus, sobald das Kabel ab ist (simuliert geprüft 08.09.2026). Die
# Helligkeit setzt die App selbst für ihr Fenster (30 %).
Write-Host "Akku: Energiesparmodus (Automatik ab 100 %, sticky)"
adb shell settings put global low_power_trigger_level 100
adb shell settings put global automatic_power_save_mode 0
adb shell settings put global low_power_sticky 1
adb shell settings put global low_power 1
# Der Sparmodus schaltet sonst den Nachtmodus mit ein (Review-Befund).
adb shell settings put global battery_saver_constants "enable_night_mode=false"

if ($OhneEntschlacken) { Write-Host "Amazon-Apps bleiben (-OhneEntschlacken)." }
else { Write-Host "Amazon-Apps entfernen (Alexa, Video, Kindle, OTA …)"; Entschlacken }

Write-Host "WebView-Stand (für die Doku/Fehlersuche):"
foreach ($p in "com.amazon.webview.chromium", "com.google.android.webview", "com.android.webview") {
  $v = adb shell dumpsys package $p | Select-String "versionName"
  if ($v) { Write-Host "  $p $v" }
}

Write-Host "App starten"
adb shell am start -n "$Paket/.KioskActivity"
Write-Host "Fertig. Am Tablet jetzt die Kiosk-PIN vergeben."
