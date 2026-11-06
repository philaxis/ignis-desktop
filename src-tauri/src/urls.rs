//! Pure URL logic: normalising the server address, building window URLs and
//! deciding what a `window.open` request from the page should turn into.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use url::Url;

/// Same escaping as JavaScript's `encodeURIComponent`, so `?file=` values
/// match what Ignis itself produces and parses.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~')
    .remove(b'!')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// Turns whatever the user typed into a canonical server URL: http(s) only,
/// no query or fragment, path ending in `/`. A missing scheme defaults to
/// `https://`.
pub fn normalize_server_url(input: &str) -> Result<Url, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Enter the address of your Ignis server.".into());
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let mut url = Url::parse(&with_scheme).map_err(|e| format!("Not a valid address: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("The address must start with http:// or https://.".into());
    }
    if url.host_str().is_none() {
        return Err("The address has no host name.".into());
    }
    url.set_query(None);
    url.set_fragment(None);
    let _ = url.set_username("");
    let _ = url.set_password(None);
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

/// Builds the URL a window loads: `<server>?vault=…&workspace=…&file=…`.
/// Absent or empty parts are left out.
pub fn window_url(
    server: &Url,
    vault: Option<&str>,
    workspace: Option<&str>,
    file: Option<&str>,
) -> Url {
    let mut url = server.clone();
    url.set_query(None);
    url.set_fragment(None);
    let query = [("vault", vault), ("workspace", workspace), ("file", file)]
        .into_iter()
        .filter_map(|(key, value)| {
            let value = value.filter(|v| !v.is_empty())?;
            Some(format!("{key}={}", utf8_percent_encode(value, COMPONENT)))
        })
        .collect::<Vec<_>>()
        .join("&");
    if !query.is_empty() {
        url.set_query(Some(&query));
    }
    url
}

/// What to do with a page's request to open a new window.
#[derive(Debug, PartialEq, Eq)]
pub enum NewWindowTarget {
    /// Open another native window on the same server.
    Native {
        vault: Option<String>,
        file: Option<String>,
    },
    /// Hand the link to the system browser.
    External(Url),
    /// Do nothing (blob:, data:, javascript:, …).
    Ignore,
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
        .filter(|v| !v.is_empty())
}

/// Classifies a `window.open` / `target=_blank` request.
///
/// * The server's own page (any query) and `about:blank` become a native
///   window; `vault` and `file` are carried over, `workspace` is not, because
///   every window gets its own.
/// * Any other http(s) or mailto URL, including other paths on the server such
///   as file downloads, goes to the system browser.
pub fn classify_new_window(server: &Url, requested: &Url) -> NewWindowTarget {
    match requested.scheme() {
        "about" if requested.as_str() == "about:blank" => NewWindowTarget::Native {
            vault: None,
            file: None,
        },
        "http" | "https" if same_origin(server, requested) && requested.path() == server.path() => {
            NewWindowTarget::Native {
                vault: query_value(requested, "vault"),
                file: query_value(requested, "file"),
            }
        }
        "http" | "https" | "mailto" => NewWindowTarget::External(requested.clone()),
        _ => NewWindowTarget::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Url {
        normalize_server_url("https://ignis.example.ts.net").unwrap()
    }

    #[test]
    fn normalize_adds_scheme_and_trailing_slash() {
        assert_eq!(
            normalize_server_url("  ignis.example.ts.net ")
                .unwrap()
                .as_str(),
            "https://ignis.example.ts.net/"
        );
    }

    #[test]
    fn normalize_keeps_http_port_and_path_prefix() {
        assert_eq!(
            normalize_server_url("http://127.0.0.1:8080")
                .unwrap()
                .as_str(),
            "http://127.0.0.1:8080/"
        );
        assert_eq!(
            normalize_server_url("https://example.com/ignis")
                .unwrap()
                .as_str(),
            "https://example.com/ignis/"
        );
    }

    #[test]
    fn normalize_strips_query_fragment_and_credentials() {
        assert_eq!(
            normalize_server_url("https://u:p@example.com/?vault=x#top")
                .unwrap()
                .as_str(),
            "https://example.com/"
        );
    }

    #[test]
    fn normalize_rejects_bad_input() {
        assert!(normalize_server_url("").is_err());
        assert!(normalize_server_url("   ").is_err());
        assert!(normalize_server_url("ftp://example.com").is_err());
        assert!(normalize_server_url("https://").is_err());
    }

    #[test]
    fn window_url_first_window_has_only_the_vault() {
        assert_eq!(
            window_url(&server(), Some("vault"), None, None).as_str(),
            "https://ignis.example.ts.net/?vault=vault"
        );
    }

    #[test]
    fn window_url_extra_window_gets_its_own_workspace() {
        assert_eq!(
            window_url(&server(), Some("vault"), Some("w2"), None).as_str(),
            "https://ignis.example.ts.net/?vault=vault&workspace=w2"
        );
    }

    #[test]
    fn window_url_encodes_note_paths_like_encode_uri_component() {
        let url = window_url(
            &server(),
            Some("My Vault"),
            Some("w3"),
            Some("notes/한글 & more.md"),
        );
        assert_eq!(
            url.as_str(),
            "https://ignis.example.ts.net/?vault=My%20Vault&workspace=w3&file=notes%2F%ED%95%9C%EA%B8%80%20%26%20more.md"
        );
        assert_eq!(
            query_value(&url, "file").as_deref(),
            Some("notes/한글 & more.md")
        );
    }

    #[test]
    fn window_url_omits_empty_parts_and_replaces_an_existing_query() {
        let base = Url::parse("https://ignis.example.ts.net/?vault=old").unwrap();
        assert_eq!(
            window_url(&base, Some(""), None, None).as_str(),
            "https://ignis.example.ts.net/"
        );
    }

    #[test]
    fn classify_server_page_opens_a_native_window_with_file() {
        let requested =
            Url::parse("https://ignis.example.ts.net/?vault=v&workspace=w9&file=a%2Fb.md").unwrap();
        assert_eq!(
            classify_new_window(&server(), &requested),
            NewWindowTarget::Native {
                vault: Some("v".into()),
                file: Some("a/b.md".into())
            }
        );
    }

    #[test]
    fn classify_about_blank_opens_a_native_window() {
        let requested = Url::parse("about:blank").unwrap();
        assert_eq!(
            classify_new_window(&server(), &requested),
            NewWindowTarget::Native {
                vault: None,
                file: None
            }
        );
    }

    #[test]
    fn classify_external_links_go_to_the_system_browser() {
        let requested = Url::parse("https://example.org/page").unwrap();
        assert_eq!(
            classify_new_window(&server(), &requested),
            NewWindowTarget::External(requested)
        );
        let same_host_other_scheme = Url::parse("http://ignis.example.ts.net/").unwrap();
        assert!(matches!(
            classify_new_window(&server(), &same_host_other_scheme),
            NewWindowTarget::External(_)
        ));
    }

    #[test]
    fn classify_other_server_paths_are_not_native_windows() {
        let requested = Url::parse("https://ignis.example.ts.net/api/download?path=x").unwrap();
        assert!(matches!(
            classify_new_window(&server(), &requested),
            NewWindowTarget::External(_)
        ));
    }

    #[test]
    fn classify_ignores_opaque_schemes() {
        for raw in [
            "blob:https://ignis.example.ts.net/abc",
            "data:text/plain,hi",
            "javascript:void(0)",
        ] {
            let requested = Url::parse(raw).unwrap();
            assert_eq!(
                classify_new_window(&server(), &requested),
                NewWindowTarget::Ignore,
                "{raw}"
            );
        }
    }
}
