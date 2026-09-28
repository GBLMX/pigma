use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Shortest interval between two freshness checks of the cookie file.
///
/// The file holds a handful of cookies, so a check is one small read; the
/// interval only exists to keep callers that run per animation frame (login
/// state, request preparation) from touching the disk on every call.
const SYNC_INTERVAL: Duration = Duration::from_millis(250);

const SAVE_INTERVAL: Duration = Duration::from_secs(10);

/// The client fingerprint cookie. It is issued once and from then on owned by
/// the file rather than by the process.
const DEVICE_ID: &str = "deviceId";

/// Generate a 52-character hexadecimal device fingerprint (matches the official client's
/// `generateDeviceId`)
fn generate_device_id() -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut s = String::with_capacity(52);
    for _ in 0..52 {
        s.push(HEX[(rand::random::<u8>() % 16) as usize] as char);
    }
    s
}

/// The persistent Cookie issued by the server after login plus the client-generated session
/// identifiers.
///
/// The instance is a *cache* of `cookies.json`, not an authority over it: every
/// boxpigma process (the TUI and `boxpigma -d`) shares that one file, so
///
/// * reads fold in what another instance has written since the last check
///   ([`Self::sync_if_due`]), which makes a login or logout performed in one
///   process visible in the others without a restart, and
/// * writes re-read the file under a cross-process lock and apply only this
///   instance's own changes ([`Self::dirty`]) on top of it, then replace it
///   atomically (temp file + rename) so no concurrent reader can ever observe a
///   truncated file.
///
/// A cookie this instance changed itself wins over the file until it has been
/// flushed; everything else follows the file. That is what keeps the processes
/// converging instead of fighting: a removal made elsewhere is adopted, not
/// rolled back, and a key another instance added is never dropped just because
/// this store had not seen it.
pub struct CookieStore {
    /// Persistent cookies (from set-cookie, serialized to disk)
    cookies: HashMap<String, String>,
    /// Cookie names this instance has set or removed since the last successful
    /// write. Values come from `cookies`; a name that is dirty but absent from
    /// `cookies` is a pending deletion.
    dirty: HashSet<String>,
    /// Session-level random identifiers (regenerated at each startup, not persisted)
    session: SessionCookies,
    /// Disk path
    path: PathBuf,
    /// The extracted CSRF token
    csrf: String,
    /// Time of the last freshness check
    last_check: Instant,
    /// Time of the last write to disk
    last_save: Instant,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Persisted {
    cookies: HashMap<String, String>,
}

struct SessionCookies {
    wn_mc_id: String,
    ntes_nnid: String,
    ntes_nuid: String,
    nmtid: String,
}

impl SessionCookies {
    fn new() -> Self {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let now = now_ms.to_string();
        let nuid = format!("{:x}{:x}", rand::random::<u64>(), rand::random::<u64>());
        Self {
            wn_mc_id: format!(
                "{:02x}{:02x}{:02x}.{}",
                rand::random::<u8>(),
                rand::random::<u8>(),
                rand::random::<u8>(),
                now,
            ),
            ntes_nnid: format!("{},{}", nuid, now),
            ntes_nuid: nuid,
            nmtid: format!("{:x}", rand::random::<u64>()),
        }
    }
}

/// Best-effort cross-process lock held in `<cookies>.lock` next to the cookie
/// file. `create_new` is atomic on every platform, so this needs no extra
/// dependency; a lock left behind by a killed process is broken after 5 s.
struct FileLock {
    path: PathBuf,
}

impl FileLock {
    fn acquire(cookie_path: &Path) -> Option<Self> {
        let path = cookie_path.with_extension("lock");
        for _ in 0..200 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Some(Self { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(5));
                    if stale {
                        let _ = std::fs::remove_file(&path);
                    } else {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                // Unwritable directory and friends: degrade to the unlocked merge.
                Err(_) => return None,
            }
        }
        None
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl CookieStore {
    pub fn new(path: PathBuf) -> Self {
        // A missing or unreadable file leaves the cache empty; a corrupt file is
        // not deleted here, the first flush keeps a copy of it.
        let mut cookies = read_persisted(&path)
            .map(|persisted| persisted.cookies)
            .unwrap_or_default();

        // Device fingerprint: generated once, persisted, and from then on owned
        // by the file so that every instance sends the same one.
        let mut dirty = HashSet::new();
        if !cookies.contains_key(DEVICE_ID) {
            cookies.insert(DEVICE_ID.to_string(), generate_device_id());
            dirty.insert(DEVICE_ID.to_string());
        }

        let csrf = cookies.get("__csrf").cloned().unwrap_or_default();

        Self {
            cookies,
            dirty,
            session: SessionCookies::new(),
            path,
            csrf,
            last_check: Instant::now(),
            last_save: Instant::now(),
        }
    }

    /// The current device fingerprint (52 hexadecimal characters)
    pub fn device_id(&self) -> &str {
        self.cookies
            .get(DEVICE_ID)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Build the full Cookie header
    pub fn build_cookie_header(&self, is_eapi: bool) -> String {
        let (os, appver, osver) = if is_eapi {
            ("iphone", "9.0.90", "16.2")
        } else {
            ("pc", "2.7.1.198277", "10")
        };

        let mut parts: Vec<String> = self
            .cookies
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Append the client-generated session identifiers
        parts.push(format!("os={}", os));
        parts.push(format!("appver={}", appver));
        parts.push(format!("osver={}", osver));
        parts.push("WEVNSM=1.0.0".to_string());
        parts.push(format!("WNMCID={}", self.session.wn_mc_id));
        parts.push(format!("_ntes_nnid={}", self.session.ntes_nnid));
        parts.push(format!("_ntes_nuid={}", self.session.ntes_nuid));
        parts.push(format!("NMTID={}", self.session.nmtid));
        parts.push("__remember_me=true".to_string());
        parts.push("channel=".to_string());

        parts.join("; ")
    }

    /// Fold in changes made by another instance, at most once per
    /// [`SYNC_INTERVAL`]. Cheap enough for per-frame callers.
    pub fn sync_if_due(&mut self) {
        if self.last_check.elapsed() >= SYNC_INTERVAL {
            self.sync_from_disk();
        }
    }

    /// Fold the current content of the cookie file into this store.
    ///
    /// Every cookie this instance has not changed itself follows the file: a
    /// value another process wrote is adopted, and a cookie that disappeared
    /// from the file was removed by another process and is dropped here too.
    /// This instance's own unsaved changes ([`Self::dirty`]) stay in memory
    /// until [`Self::flush`] has written them.
    pub fn sync_from_disk(&mut self) {
        self.last_check = Instant::now();
        let Some(disk) = read_persisted(&self.path).map(|persisted| persisted.cookies) else {
            // Missing or corrupt file: keep the in-memory state. The first flush
            // keeps a copy of an unreadable file before replacing it.
            return;
        };

        self.adopt_device_id(&disk);

        let mut adopted = 0usize;
        for (name, value) in &disk {
            if self.dirty.contains(name) {
                continue;
            }
            if self.cookies.get(name) != Some(value) {
                self.cookies.insert(name.clone(), value.clone());
                adopted += 1;
            }
        }

        // Cookies that are gone from the file were removed by another instance.
        let dirty = &self.dirty;
        let before = self.cookies.len();
        self.cookies
            .retain(|name, _| dirty.contains(name) || disk.contains_key(name));
        let removed = before - self.cookies.len();

        if adopted > 0 || removed > 0 {
            log::debug!(
                "cookies: adopted {adopted} value(s) and {removed} removal(s) from another instance"
            );
            self.adopt_csrf();
        }
    }

    /// Extract Set-Cookie from the response headers
    pub fn update_from_response(&mut self, headers: &reqwest::header::HeaderMap) {
        // Fold in other instances' writes first, so the server's fresh values are
        // applied on top of the current state instead of an outdated one.
        self.sync_if_due();

        let mut changed = false;

        for set_cookie in headers.get_all("set-cookie").iter() {
            let Ok(val) = set_cookie.to_str() else {
                continue;
            };
            let Some(cookie_part) = val.split(';').next() else {
                continue;
            };
            let Some((name, value)) = cookie_part.split_once('=') else {
                continue;
            };
            let name = name.trim().to_string();
            let value = value.trim().to_string();

            // Track __csrf
            if name == "__csrf" && !value.is_empty() {
                self.csrf = value.clone();
            }

            if self.cookies.get(&name) != Some(&value) {
                changed = true;
                self.dirty.insert(name.clone());
                self.cookies.insert(name, value);
            }
        }

        if changed {
            self.flush_if_stale();
        }
    }

    /// CSRF token
    pub fn csrf_token(&self) -> &str {
        &self.csrf
    }

    /// Whether the user is logged in (based on whether the key cookie is present)
    pub fn is_logged_in(&self) -> bool {
        self.cookies.contains_key("MUSIC_U") || self.cookies.contains_key("__csrf")
    }

    /// Remove the specified cookie
    pub fn remove(&mut self, name: &str) {
        // Reflect concurrent writes before recording our own removal, so both
        // changes end up merged with the file instead of fighting over it.
        self.sync_if_due();
        self.cookies.remove(name);
        self.dirty.insert(name.to_string());
    }

    /// Force a write to disk.
    ///
    /// The file is re-read under a cross-process lock and only this instance's
    /// changes are applied on top of it, then the result replaces the file
    /// atomically. Nothing is written when this instance has no pending change,
    /// the file then already is the truth (and is adopted as the new cache).
    pub fn flush(&mut self) {
        // Serialize with the other instances (the TUI plus `boxpigma -d`):
        // without this, two instances flushing inside the same read-modify-write
        // window silently drop each other's freshly written keys. Best effort —
        // if the lock cannot be taken, our own keys are still merged in.
        let _lock = FileLock::acquire(&self.path);

        let on_disk = read_persisted(&self.path);
        let readable = on_disk.is_some();
        let mut next = on_disk
            .map(|persisted| persisted.cookies)
            .unwrap_or_default();

        // `deviceId` belongs to the file: adopt the persisted fingerprint instead
        // of replacing it with the one this instance generated.
        self.adopt_device_id(&next);

        // Without a readable file there is nothing to merge against: write the
        // whole in-memory state. Otherwise write exactly our own changes.
        let names: Vec<String> = if readable {
            self.dirty.iter().cloned().collect()
        } else {
            self.cookies.keys().cloned().collect()
        };
        for name in &names {
            match self.cookies.get(name) {
                Some(value) => {
                    next.insert(name.clone(), value.clone());
                }
                // Dirty but absent from the cache: a pending deletion wins over
                // whatever the file still has.
                None => {
                    next.remove(name);
                }
            }
        }

        let wrote = if readable && self.dirty.is_empty() {
            // Nothing of ours to write; the file stays as it is.
            false
        } else {
            if !readable && self.path.exists() {
                back_up_unreadable(&self.path);
            }
            match write_persisted(&self.path, &next) {
                Ok(()) => true,
                Err(e) => {
                    log::warn!("failed to write cookie file {:?}: {}", self.path, e);
                    false
                }
            }
        };

        if wrote || readable {
            // Adopt the file as the new cache: everything another instance wrote
            // is now visible here, and a removal made elsewhere is not rolled
            // back by our next flush.
            self.cookies = next;
            self.adopt_csrf();
        }
        if wrote {
            self.dirty.clear();
        }
        self.last_check = Instant::now();
        self.last_save = Instant::now();
    }

    /// `deviceId` is issued once and then owned by the file: take the persisted
    /// fingerprint and give up our own pending write for it, so two instances
    /// that started at the same time end up sending the same one.
    fn adopt_device_id(&mut self, disk: &HashMap<String, String>) {
        if let Some(id) = disk.get(DEVICE_ID) {
            if self.cookies.get(DEVICE_ID) != Some(id) {
                self.cookies.insert(DEVICE_ID.to_string(), id.clone());
            }
            self.dirty.remove(DEVICE_ID);
        }
    }

    /// The cached CSRF token follows the last non-empty `__csrf` value seen, from
    /// a response or from another instance's write that was adopted here.
    fn adopt_csrf(&mut self) {
        if let Some(value) = self.cookies.get("__csrf")
            && !value.is_empty()
            && *value != self.csrf
        {
            self.csrf = value.clone();
        }
    }

    fn flush_if_stale(&mut self) {
        if self.last_save.elapsed() >= SAVE_INTERVAL {
            self.flush();
        }
    }
}

/// Read and parse the persisted cookie file, returning `None` when it is
/// missing or cannot be parsed.
fn read_persisted(path: &Path) -> Option<Persisted> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// Move an unreadable cookie file aside so the flush that follows does not
/// silently discard whatever it contained.
fn back_up_unreadable(path: &Path) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup = path.with_extension(format!("corrupt-{stamp}"));
    match std::fs::rename(path, &backup) {
        Ok(()) => log::warn!(
            "cookie file {:?} could not be parsed; kept a copy at {:?}",
            path,
            backup
        ),
        Err(e) => log::warn!(
            "cookie file {:?} could not be parsed and could not be kept aside: {}",
            path,
            e
        ),
    }
}

/// Atomically write the persisted cookies to `path`.
///
/// A unique temp file is written in the same directory (mode `0600` on Unix)
/// and then renamed over the target — `rename` is atomic, so a concurrent
/// reader always sees either the old or the new complete file.
fn write_persisted(path: &Path, cookies: &HashMap<String, String>) -> std::io::Result<()> {
    let persisted = Persisted {
        cookies: cookies.clone(),
    };
    let json = serde_json::to_string_pretty(&persisted)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("cookies.json");
    let tmp = dir.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        rand::random::<u32>()
    ));

    if let Err(e) = write_temp_file(&tmp, json.as_bytes()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

fn write_temp_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()
}

impl Drop for CookieStore {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_cookies(path: &std::path::Path) -> Persisted {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn set_cookie(store: &mut CookieStore, name: &str, value: &str) {
        let mut h = reqwest::header::HeaderMap::new();
        h.append(
            "set-cookie",
            format!("{name}={value}; Path=/").parse().unwrap(),
        );
        store.update_from_response(&h);
    }

    fn write_cookies(path: &Path, cookies: HashMap<String, String>) {
        let json = serde_json::to_string_pretty(&Persisted { cookies }).unwrap();
        std::fs::write(path, json).unwrap();
    }

    /// Another instance logs out: this stale instance must not roll that
    /// removal back on a later flush.
    #[test]
    fn external_removal_is_not_rolled_back() {
        let path = temp_cookie_path();
        let mut s = CookieStore::new(path.clone());
        set_cookie(&mut s, "MUSIC_U", "aaaa");
        set_cookie(&mut s, "__csrf", "cccc");
        s.flush();

        // Simulate the other instance's logout: rewrite the file without MUSIC_U.
        let mut disk = read_cookies(&path).cookies;
        disk.remove("MUSIC_U");
        write_cookies(&path, disk);

        s.flush();
        s.flush();
        let second = read_cookies(&path).cookies;
        assert!(
            !second.contains_key("MUSIC_U"),
            "a stale instance rolled the other instance's logout back"
        );
        assert_eq!(second.get("__csrf").map(String::as_str), Some("cccc"));
        assert!(
            !s.cookies.contains_key("MUSIC_U"),
            "the store kept a cookie that another instance removed"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// A login performed by another instance becomes visible here without a
    /// restart.
    #[test]
    fn another_instances_login_is_adopted() {
        let path = temp_cookie_path();
        // B is already running, before anything has ever been written.
        let mut b = CookieStore::new(path.clone());
        assert!(!b.is_logged_in());

        // A logs in and persists.
        let mut a = CookieStore::new(path.clone());
        set_cookie(&mut a, "MUSIC_U", "token");
        set_cookie(&mut a, "__csrf", "csrf");
        a.flush();

        b.sync_from_disk();
        assert!(b.is_logged_in(), "B did not notice A's login");
        assert_eq!(b.csrf_token(), "csrf");
        assert_eq!(
            b.device_id(),
            a.device_id(),
            "the fingerprint must converge"
        );
        assert!(b.build_cookie_header(false).contains("MUSIC_U=token"));
        let _ = std::fs::remove_file(path);
    }

    /// A logout performed by another instance is adopted, and the stale instance
    /// does not write the cookie back.
    #[test]
    fn another_instances_logout_is_adopted() {
        let path = temp_cookie_path();
        let mut a = CookieStore::new(path.clone());
        set_cookie(&mut a, "MUSIC_U", "token");
        set_cookie(&mut a, "__csrf", "csrf");
        a.flush();

        // B is already up and logged in.
        let mut b = CookieStore::new(path.clone());
        assert!(b.is_logged_in());

        a.remove("MUSIC_U");
        a.flush();

        b.sync_from_disk();
        assert!(
            !b.cookies.contains_key("MUSIC_U"),
            "B still holds the token"
        );
        b.flush();
        let disk = read_cookies(&path).cookies;
        assert!(
            !disk.contains_key("MUSIC_U"),
            "the stale instance resurrected the login"
        );
        assert_eq!(disk.get("__csrf").map(String::as_str), Some("csrf"));
        let _ = std::fs::remove_file(path);
    }

    /// Two instances flushing at the same time must not drop each other's keys.
    #[test]
    fn concurrent_instances_do_not_lose_keys() {
        fn keys(tag: &'static str) -> Vec<String> {
            (0..60).map(|i| format!("{tag}{i}")).collect()
        }

        let path = temp_cookie_path();
        let spawn = |tag: &'static str| {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut store = CookieStore::new(path);
                store.flush();
                for (i, name) in keys(tag).into_iter().enumerate() {
                    set_cookie(&mut store, &name, &format!("v{i}"));
                    store.flush();
                }
                store
            })
        };
        let ta = spawn("a");
        let tb = spawn("b");
        let _a = ta.join().unwrap();
        let _b = tb.join().unwrap();

        let disk = read_cookies(&path).cookies;
        let missing: Vec<String> = keys("a")
            .into_iter()
            .chain(keys("b"))
            .filter(|k| !disk.contains_key(k))
            .collect();
        assert!(
            missing.is_empty(),
            "keys lost to a concurrent writer: {missing:?}"
        );
        assert!(disk.contains_key("deviceId"), "file must stay readable");
        assert_eq!(
            disk.get("deviceId").map(String::as_str),
            Some(_a.device_id()),
            "both instances must agree on the fingerprint"
        );
        let _ = std::fs::remove_file(&path);
    }

    fn temp_cookie_path() -> PathBuf {
        let dir = std::env::temp_dir().join("ncm_cookie_test");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("test_{}.json", rand::random::<u64>()))
    }

    #[test]
    fn test_new_store_creates_empty() {
        let path = temp_cookie_path();
        let store = CookieStore::new(path.clone());
        assert!(!store.is_logged_in());
        assert_eq!(store.csrf_token(), "");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_new_store_loads_existing() {
        let path = temp_cookie_path();
        let mut cookies = HashMap::new();
        cookies.insert("MUSIC_U".to_string(), "test_token".to_string());
        cookies.insert("__csrf".to_string(), "csrf123".to_string());
        write_cookies(&path, cookies);

        let store = CookieStore::new(path.clone());
        assert!(store.is_logged_in());
        assert_eq!(store.csrf_token(), "csrf123");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_flush_persists_cookies() {
        let path = temp_cookie_path();
        let mut store = CookieStore::new(path.clone());
        // The first flush creates the file with the generated fingerprint.
        store.flush();
        assert!(read_cookies(&path).cookies.contains_key(DEVICE_ID));

        set_cookie(&mut store, "key", "value");
        // A cookie this instance did not fetch is not ours to write.
        store
            .cookies
            .insert("borrowed".to_string(), "v".to_string());
        store.flush();

        let loaded = read_cookies(&path).cookies;
        assert_eq!(loaded.get("key").map(String::as_str), Some("value"));
        assert!(!loaded.contains_key("borrowed"));
        assert!(loaded.contains_key(DEVICE_ID));
        let _ = std::fs::remove_file(path);
    }

    /// If the file disappears while this instance is running, the next flush
    /// restores the in-memory state instead of writing an empty file.
    #[test]
    fn a_deleted_file_is_restored_from_memory() {
        let path = temp_cookie_path();
        let mut store = CookieStore::new(path.clone());
        set_cookie(&mut store, "MUSIC_U", "token");
        store.flush();

        let _ = std::fs::remove_file(&path);
        store.flush();

        assert_eq!(
            read_disk(&path).get("MUSIC_U").map(String::as_str),
            Some("token")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_build_cookie_header_contains_session() {
        let path = temp_cookie_path();
        let store = CookieStore::new(path.clone());
        let header = store.build_cookie_header(false);
        assert!(header.contains("os=pc"));
        assert!(header.contains("WNMCID="));
        assert!(header.contains("_ntes_nnid="));
        assert!(header.contains("__remember_me=true"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_build_cookie_header_eapi() {
        let path = temp_cookie_path();
        let store = CookieStore::new(path.clone());
        let header = store.build_cookie_header(true);
        assert!(header.contains("os=iphone"));
        assert!(header.contains("appver=9.0.90"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_new_store_generates_device_id() {
        let path = temp_cookie_path();
        let store = CookieStore::new(path.clone());
        let id = store.device_id().to_string();
        assert_eq!(id.len(), 52);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        );

        // The header carries the fingerprint
        let header = store.build_cookie_header(false);
        assert!(header.contains(&format!("deviceId={id}")));

        // Persisted so it stays consistent across restarts
        let mut store = store;
        store.flush();
        let store2 = CookieStore::new(path.clone());
        assert_eq!(store2.device_id(), id);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_is_logged_in_music_u() {
        let path = temp_cookie_path();
        let mut store = CookieStore::new(path.clone());
        store
            .cookies
            .insert("MUSIC_U".to_string(), "token".to_string());
        assert!(store.is_logged_in());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_is_logged_in_csrf() {
        let path = temp_cookie_path();
        let mut store = CookieStore::new(path.clone());
        store
            .cookies
            .insert("__csrf".to_string(), "csrf".to_string());
        assert!(store.is_logged_in());
        let _ = std::fs::remove_file(path);
    }

    fn read_disk(path: &Path) -> HashMap<String, String> {
        read_persisted(path).map(|p| p.cookies).unwrap_or_default()
    }

    /// A second instance must neither wipe a session it never fetched nor
    /// replace the fingerprint stored in the file.
    #[test]
    fn a_second_instance_does_not_clobber_the_session() {
        let path = temp_cookie_path();

        // Instance A logs in and persists a session.
        let mut a = CookieStore::new(path.clone());
        set_cookie(&mut a, "MUSIC_U", "token");
        a.flush();
        let device = a.device_id().to_string();

        // Instance B starts (it sees A's file) and shuts down.
        let mut b = CookieStore::new(path.clone());
        assert_eq!(b.device_id(), device);
        b.flush();

        let disk = read_disk(&path);
        assert_eq!(
            disk.get("MUSIC_U").map(String::as_str),
            Some("token"),
            "instance B wiped instance A's login cookie"
        );
        assert_eq!(
            disk.get("deviceId").map(String::as_str),
            Some(device.as_str()),
            "a persisted deviceId must not be replaced by a generated one"
        );

        // Instance C started while no file existed, so it generated a fingerprint
        // of its own (like an instance that started mid-write): flushing must
        // adopt A's instead of replacing it.
        let _ = std::fs::remove_file(&path);
        let mut c = CookieStore::new(path.clone());
        assert_ne!(c.device_id(), device);
        let mut restored = HashMap::new();
        restored.insert("deviceId".to_string(), device.clone());
        restored.insert("MUSIC_U".to_string(), "token".to_string());
        write_cookies(&path, restored);

        c.flush();
        let disk = read_disk(&path);
        assert_eq!(
            disk.get("deviceId").map(String::as_str),
            Some(device.as_str()),
            "a generated deviceId replaced the persisted one"
        );
        assert_eq!(disk.get("MUSIC_U").map(String::as_str), Some("token"));
        assert_eq!(
            c.device_id(),
            device,
            "the store must adopt the file's fingerprint"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn removal_is_persisted_without_dropping_unrelated_cookies() {
        let path = temp_cookie_path();
        let mut a = CookieStore::new(path.clone());
        set_cookie(&mut a, "MUSIC_U", "token");
        set_cookie(&mut a, "__csrf", "csrf");
        a.flush();

        // A second instance logs out: only MUSIC_U is removed.
        let mut b = CookieStore::new(path.clone());
        b.remove("MUSIC_U");
        b.flush();

        let disk = read_disk(&path);
        assert!(!disk.contains_key("MUSIC_U"));
        assert_eq!(disk.get("__csrf").map(String::as_str), Some("csrf"));

        let _ = std::fs::remove_file(path);
    }

    /// An unparseable file is kept aside instead of being discarded.
    #[test]
    fn an_unreadable_file_is_kept_aside() {
        let path = temp_cookie_path();
        std::fs::write(&path, b"{ this is not json").unwrap();

        let mut store = CookieStore::new(path.clone());
        set_cookie(&mut store, "MUSIC_U", "token");
        store.flush();

        assert_eq!(
            read_disk(&path).get("MUSIC_U").map(String::as_str),
            Some("token")
        );

        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let backup = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(&format!("{stem}.corrupt-")))
            })
            .expect("the unreadable file was discarded");
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "{ this is not json"
        );
        let _ = std::fs::remove_file(&backup);
        let _ = std::fs::remove_file(path);
    }
}
