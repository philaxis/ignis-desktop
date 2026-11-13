package dev.philaxis.ignis

/**
 * Every app window (an Android task; two of them can sit side by side in
 * split screen) has an id, which is also its Ignis workspace, so each window
 * keeps its own tabs. Ids start at `tablet` and never collide with the
 * desktop app's `main`, `w2`, … or with phones that use Ignis without one.
 */
object WindowIds {
    const val FIRST = "tablet"

    /** The lowest id not in use, so a closed window's layout comes back. */
    fun next(inUse: Set<String>): String =
        generateSequence(1) { it + 1 }
            .map { if (it == 1) FIRST else "$FIRST-$it" }
            .first { it !in inUse }
}
