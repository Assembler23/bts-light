package de.badhub.btslight.tablet.kern

/** Versionsregel der App — dieselbe Formel wie in app/build.gradle.kts. */
object Version {
    /** `major*1_000_000 + minor*10_000 + patch`, damit der Code streng mit der Version steigt. */
    fun versionCode(versionName: String): Int {
        val t = versionName.split(".").map { it.toInt() }
        return t[0] * 1_000_000 + t[1] * 10_000 + t[2]
    }
}
