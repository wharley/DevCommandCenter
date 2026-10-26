//! The imported cookies into the Browser's own WebKit store (the desktop's
//! `dcc_browser_set_session_cookies`, here in Rust): each becomes an
//! `NSHTTPCookie` (Secure and `HttpOnly` only when set, the expiry, the
//! SameSite policy) set on the page's `httpCookieStore`; then the store is
//! read back and only the cookies it really holds count as imported.

use dcc_infra::browser_sessions::PreparedSessionCookie;

/// Imported and skipped, once WebKit answered.
pub type Done = Box<dyn FnOnce(usize, usize)>;

#[cfg(target_os = "macos")]
pub fn set_cookies(
    webview: &objc2_web_kit::WKWebView,
    cookies: Vec<PreparedSessionCookie>,
    done: Done,
) {
    use std::cell::{Cell, RefCell};
    use std::ptr::NonNull;
    use std::rc::Rc;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_foundation::{NSArray, NSHTTPCookie};

    let mut built: Vec<Retained<NSHTTPCookie>> = Vec::new();
    let mut rejected = 0;
    for cookie in &cookies {
        match build(cookie) {
            Some(cookie) => built.push(cookie),
            None => rejected += 1,
        }
    }
    if built.is_empty() {
        done(0, rejected);
        return;
    }
    let store = unsafe { webview.configuration().websiteDataStore().httpCookieStore() };
    let expected = Rc::new(built);
    let remaining = Rc::new(Cell::new(expected.len()));
    let done = Rc::new(RefCell::new(Some(done)));
    for cookie in expected.iter() {
        let (remaining, expected, done, reader) = (
            remaining.clone(),
            expected.clone(),
            done.clone(),
            store.clone(),
        );
        let set = RcBlock::new(move || {
            remaining.set(remaining.get().saturating_sub(1));
            if remaining.get() > 0 {
                return;
            }
            let (expected, done) = (expected.clone(), done.clone());
            let count = RcBlock::new(move |stored: NonNull<NSArray<NSHTTPCookie>>| {
                let stored = unsafe { stored.as_ref() };
                let imported = expected
                    .iter()
                    .filter(|wanted| stored.iter().any(|actual| same(wanted, &actual)))
                    .count();
                if let Some(done) = done.borrow_mut().take() {
                    done(imported, rejected + expected.len() - imported);
                }
            });
            unsafe { reader.getAllCookies(&count) };
        });
        unsafe { store.setCookie_completionHandler(cookie, Some(&set)) };
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_cookies(_: &(), cookies: Vec<PreparedSessionCookie>, done: Done) {
    done(0, cookies.len());
}

#[cfg(target_os = "macos")]
fn build(
    cookie: &PreparedSessionCookie,
) -> Option<objc2::rc::Retained<objc2_foundation::NSHTTPCookie>> {
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSCopying;
    use objc2_foundation::{
        NSDate, NSHTTPCookie, NSHTTPCookieDomain, NSHTTPCookieExpires, NSHTTPCookieName,
        NSHTTPCookiePath, NSHTTPCookieSameSiteLax, NSHTTPCookieSameSitePolicy,
        NSHTTPCookieSameSiteStrict, NSHTTPCookieSecure, NSHTTPCookieValue, NSMutableDictionary,
        NSNumber, NSString,
    };

    let properties = NSMutableDictionary::<NSString, AnyObject>::new();
    let text = |value: &str| NSString::from_str(value);
    unsafe {
        properties.insert(NSHTTPCookieName, text(&cookie.name).as_ref());
        properties.insert(NSHTTPCookieValue, text(&cookie.value).as_ref());
        properties.insert(NSHTTPCookieDomain, text(&cookie.domain).as_ref());
        properties.insert(NSHTTPCookiePath, text(&cookie.path).as_ref());
        // Its presence alone makes the cookie Secure.
        if cookie.secure {
            properties.insert(NSHTTPCookieSecure, NSNumber::numberWithBool(true).as_ref());
        }
        // No public key for it; WebKit takes the attribute's own spelling.
        if cookie.http_only {
            properties.insert(&*text("HttpOnly"), NSNumber::numberWithBool(true).as_ref());
        }
        if let Some(expires) = cookie.expires_unix_seconds {
            let date = NSDate::dateWithTimeIntervalSince1970(expires as f64);
            properties.insert(NSHTTPCookieExpires, date.as_ref());
        }
        let policy = match cookie.same_site {
            0 => Some(text("none")),
            1 => Some(NSHTTPCookieSameSiteLax.copy()),
            2 => Some(NSHTTPCookieSameSiteStrict.copy()),
            _ => None,
        };
        if let Some(policy) = policy {
            properties.insert(NSHTTPCookieSameSitePolicy, policy.as_ref());
        }
        NSHTTPCookie::cookieWithProperties(&properties)
    }
}

/// The store kept it: the same name, value, effective domain, path, flags
/// and expiry (WebKit may drop a domain's leading dot).
#[cfg(target_os = "macos")]
fn same(wanted: &objc2_foundation::NSHTTPCookie, actual: &objc2_foundation::NSHTTPCookie) -> bool {
    let domain = |cookie: &objc2_foundation::NSHTTPCookie| {
        cookie.domain().to_string().trim_matches('.').to_string()
    };
    let expires = |cookie: &objc2_foundation::NSHTTPCookie| {
        cookie
            .expiresDate()
            .map(|date| date.timeIntervalSince1970())
    };
    wanted.name().to_string() == actual.name().to_string()
        && wanted.value().to_string() == actual.value().to_string()
        && domain(wanted) == domain(actual)
        && wanted.path().to_string() == actual.path().to_string()
        && wanted.isSecure() == actual.isSecure()
        && wanted.isHTTPOnly() == actual.isHTTPOnly()
        && match (expires(wanted), expires(actual)) {
            (None, None) => true,
            (Some(left), Some(right)) => (left - right).abs() <= 1.0,
            _ => false,
        }
}
