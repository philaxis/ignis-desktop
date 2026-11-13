package dev.philaxis.ignis

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import java.net.URI

class UrlsTest {
    private val server = Urls.normalizeServer("https://ignis.example.ts.net:8446")

    @Test
    fun normalizeAddsSchemeAndTrailingSlashAndDropsTheRest() {
        assertEquals("https://ignis.example.ts.net/", Urls.normalizeServer("  ignis.example.ts.net ").toString())
        assertEquals("http://127.0.0.1:8080/", Urls.normalizeServer("http://127.0.0.1:8080").toString())
        assertEquals("https://example.com/ignis/", Urls.normalizeServer("https://example.com/ignis").toString())
        assertEquals("https://example.com/", Urls.normalizeServer("https://u:p@example.com/?vault=x#top").toString())
    }

    @Test
    fun normalizeRejectsBadInput() {
        for (bad in listOf("", "   ", "ftp://example.com", "https://")) {
            assertThrows(bad, Urls.InvalidServer::class.java) { Urls.normalizeServer(bad) }
        }
    }

    @Test
    fun windowUrlEncodesLikeEncodeUriComponent() {
        assertEquals(
            "https://ignis.example.ts.net:8446/?vault=My%20Vault&workspace=tablet-2&file=notes%2F%ED%95%9C%EA%B8%80%20%26%20more.md",
            Urls.windowUrl(server, "My Vault", "tablet-2", "notes/한글 & more.md"),
        )
        assertEquals("https://ignis.example.ts.net:8446/?workspace=tablet", Urls.windowUrl(server, "", "tablet", null))
    }

    @Test
    fun originIsWhatWebViewOriginRulesExpect() {
        assertEquals("https://ignis.example.ts.net:8446", Urls.origin(server))
        assertEquals("https://ignis.example.ts.net", Urls.origin(Urls.normalizeServer("ignis.example.ts.net")))
    }

    @Test
    fun serverPageOpensAWindowWithItsFile() {
        assertEquals(
            Urls.NewWindowTarget.Window("v", "a/b c.md"),
            Urls.classifyNewWindow(server, "https://ignis.example.ts.net:8446/?vault=v&workspace=w9&file=a%2Fb%20c.md"),
        )
        assertEquals(
            Urls.NewWindowTarget.Window(null, null),
            Urls.classifyNewWindow(server, "https://ignis.example.ts.net:8446/?ignis-app=new-window"),
        )
    }

    @Test
    fun otherPagesGoToOtherAppsAndOpaqueUrlsAreIgnored() {
        for (external in listOf(
            "https://example.org/page",
            "https://ignis.example.ts.net/",
            "https://ignis.example.ts.net:8446/api/download?path=x",
            "mailto:me@example.org",
        )) {
            assertEquals(external, Urls.NewWindowTarget.External(external), Urls.classifyNewWindow(server, external))
        }
        for (ignored in listOf("about:blank", "blob:https://ignis.example.ts.net:8446/abc", "javascript:void(0)")) {
            assertEquals(ignored, Urls.NewWindowTarget.Ignore, Urls.classifyNewWindow(server, ignored))
        }
    }

    @Test
    fun launcherCarriesTheTargetAsOneParameter() {
        val target = Urls.windowUrl(server, "my vault", "tablet", null)
        val launcher = URI(Urls.launcherUrl(target))
        assertEquals("/assets/connect.html", launcher.path)
        assertEquals("target=$target", launcher.query)
    }
}
