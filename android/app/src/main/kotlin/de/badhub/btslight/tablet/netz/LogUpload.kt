package de.badhub.btslight.tablet.netz

import de.badhub.btslight.tablet.kern.LogPuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

/**
 * Schickt das Geräte-Log an den Turnier-PC (`POST /pi-log`), der es lokal
 * ablegt und in die Cloud weiterreicht — derselbe Weg wie bei den Pis:
 * plain HTTP im LAN, keine Uhr, kein TLS nötig. Fehler sind still.
 */
class LogUpload(private val puffer: LogPuffer, private val port: Int = 8088) {
    suspend fun sende(ip: String, geraeteId: String): Boolean = withContext(Dispatchers.IO) {
        var c: HttpURLConnection? = null
        try {
            c = URL("http://$ip:$port/pi-log?device=$geraeteId").openConnection() as HttpURLConnection
            c.connectTimeout = 8000
            c.readTimeout = 8000
            c.requestMethod = "POST"
            c.doOutput = true
            c.setRequestProperty("Content-Type", "text/plain; charset=utf-8")
            c.outputStream.use { it.write(puffer.inhalt().toByteArray(Charsets.UTF_8)) }
            c.responseCode == 200
        } catch (e: Exception) {
            false
        } finally {
            c?.disconnect()
        }
    }
}
