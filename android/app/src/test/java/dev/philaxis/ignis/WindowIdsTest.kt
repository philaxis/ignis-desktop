package dev.philaxis.ignis

import org.junit.Assert.assertEquals
import org.junit.Test

class WindowIdsTest {
    @Test
    fun takesTheLowestFreeIdSoClosedLayoutsComeBack() {
        assertEquals("tablet", WindowIds.next(emptySet()))
        assertEquals("tablet-2", WindowIds.next(setOf("tablet")))
        assertEquals("tablet-3", WindowIds.next(setOf("tablet", "tablet-2")))
        assertEquals("tablet", WindowIds.next(setOf("tablet-2")))
        assertEquals("tablet-2", WindowIds.next(setOf("tablet", "tablet-3")))
    }
}
