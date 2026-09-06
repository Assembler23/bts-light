package de.badhub.btslight.tablet.kiosk

import android.app.admin.DeviceAdminReceiver

/** Ziel von `dpm set-device-owner …/.KioskAdminReceiver`. Keine eigene Logik. */
class KioskAdminReceiver : DeviceAdminReceiver()
