package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertEquals
import org.junit.Test

class VersionTest {
    @Test
    fun versionCode_ordnet_versionen_streng_aufsteigend() {
        // Android verlangt einen monoton steigenden versionCode; 0.9.281 muss
        // vor 0.10.0 liegen, obwohl "281" > "0" ist.
        assertEquals(90_281, Version.versionCode("0.9.281"))
        assertEquals(100_000, Version.versionCode("0.10.0"))
        assertEquals(1_000_000, Version.versionCode("1.0.0"))
    }
}
