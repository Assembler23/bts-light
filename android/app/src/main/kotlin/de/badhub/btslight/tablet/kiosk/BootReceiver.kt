package de.badhub.btslight.tablet.kiosk

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import de.badhub.btslight.tablet.KioskActivity

/**
 * Rückfall-Autostart; der eigentliche Autostart ist der Home-Launcher,
 * Fire OS trödelt bei BOOT_COMPLETED.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
        val start = Intent(context, KioskActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(start)
    }
}
