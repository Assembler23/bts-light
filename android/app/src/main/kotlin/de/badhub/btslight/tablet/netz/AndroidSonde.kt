package de.badhub.btslight.tablet.netz

import de.badhub.btslight.tablet.suche.HealthAntwort
import de.badhub.btslight.tablet.suche.Sonde
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

/** Klopft an `http://<ip>:8088/health`. Kurzes Timeout, damit 254 Sonden schnell durch sind. */
class AndroidSonde(private val port: Int = 8088, private val timeoutMs: Int = 1000) : Sonde {
    override suspend fun antwortet(ip: String): Boolean = withContext(Dispatchers.IO) {
        var c: HttpURLConnection? = null
        try {
            c = URL("http://$ip:$port/health").openConnection() as HttpURLConnection
            c.connectTimeout = timeoutMs
            c.readTimeout = timeoutMs * 2
            c.requestMethod = "GET"
            val status = c.responseCode
            val body = if (status == 200) c.inputStream.bufferedReader().use { it.readText() } else ""
            HealthAntwort.istTreffer(status, body)
        } catch (e: Exception) {
            false
        } finally {
            c?.disconnect()
        }
    }
}
