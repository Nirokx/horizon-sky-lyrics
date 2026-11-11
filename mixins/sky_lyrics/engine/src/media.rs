//! What is playing (MPRIS through `playerctl`) and its lyrics (your `.lrc`
//! files, the cache, then LRCLIB), each on a background thread.

use crate::lrc::{self, Lyrics};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NowPlaying {
    pub player: String,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub length_ms: u32,
    pub position_ms: u32,
    pub playing: bool,
}

impl NowPlaying {
    pub fn key(&self) -> String {
        format!("{}\u{1f}{}", self.artist.to_lowercase(), self.title.to_lowercase())
    }
}

const FORMAT: &str = "{{playerName}}\t{{status}}\t{{artist}}\t{{title}}\t{{album}}\t{{mpris:length}}\t{{position}}";

fn micros_to_ms(text: &str) -> u32 {
    (text.trim().parse::<u64>().unwrap_or(0) / 1000).min(u64::from(u32::MAX)) as u32
}

/// A browser tab reports "Artist - Song | Site" as its title, with no artist.
fn tidy(mut np: NowPlaying) -> NowPlaying {
    if np.artist.is_empty() {
        let mut title = np.title.as_str();
        for site in [" | TikTok", " - YouTube Music", " - YouTube", " | Spotify"] {
            title = title.strip_suffix(site).unwrap_or(title);
        }
        if let Some((artist, song)) = title.split_once(" - ") {
            np.artist = artist.trim().to_owned();
            np.title = song.trim().to_owned();
        } else {
            np.title = title.trim().to_owned();
        }
    }
    np
}

/// Every player's name and the one to follow: the chosen one, or in `auto`
/// the one playing, Spotify first.
pub fn choose(output: &str, wanted: &str) -> (Vec<String>, Option<NowPlaying>) {
    let mut all: Vec<NowPlaying> = Vec::new();
    for row in output.lines() {
        let f: Vec<&str> = row.split('\t').collect();
        if f.len() < 7 {
            continue;
        }
        all.push(NowPlaying {
            player: f[0].to_owned(),
            playing: f[1] == "Playing",
            artist: f[2].trim().to_owned(),
            title: f[3].trim().to_owned(),
            album: f[4].trim().to_owned(),
            length_ms: micros_to_ms(f[5]),
            position_ms: micros_to_ms(f[6]),
        });
    }
    let names: Vec<String> = all.iter().map(|p| p.player.clone()).collect();
    let usable = |p: &&NowPlaying| !p.title.is_empty();
    let chosen = if wanted != "auto" {
        all.iter().filter(usable).find(|p| p.player == wanted)
    } else {
        let spotify = |p: &&NowPlaying| p.player.starts_with("spotify");
        all.iter()
            .filter(usable)
            .filter(|p| p.playing)
            .find(spotify)
            .or_else(|| all.iter().filter(usable).find(|p| p.playing))
            .or_else(|| all.iter().filter(usable).find(spotify))
            .or_else(|| all.iter().find(usable))
    };
    (names, chosen.cloned().map(tidy))
}

/// The song's position between polls.
#[derive(Clone, Debug, Default)]
pub struct Clock {
    key: String,
    base_ms: u32,
    at: Option<Instant>,
    playing: bool,
    last_poll: Option<Instant>,
}

impl Clock {
    pub fn now(&self, now: Instant) -> u32 {
        let Some(at) = self.at else {
            return 0;
        };
        if self.playing {
            self.base_ms.saturating_add(now.saturating_duration_since(at).as_millis() as u32)
        } else {
            self.base_ms
        }
    }

    /// Takes a poll made at `polled`; the same poll again changes nothing, so
    /// this can be called every frame. Small differences are ignored so the
    /// words don't jitter.
    pub fn update(&mut self, np: &NowPlaying, polled: Instant) {
        if self.last_poll == Some(polled) {
            return;
        }
        self.last_poll = Some(polled);
        let key = np.key();
        let drift = self.now(polled).abs_diff(np.position_ms);
        if key != self.key || self.at.is_none() || self.playing != np.playing || drift > 350 {
            self.key = key;
            self.base_ms = np.position_ms;
            self.at = Some(polled);
            self.playing = np.playing;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Found {
    #[default]
    Nothing,
    Searching,
    Lyrics(Arc<Lyrics>),
    Instrumental,
    NotFound,
    Offline,
}

impl Found {
    pub fn describe(&self) -> &'static str {
        match self {
            Found::Nothing => "Nothing playing",
            Found::Searching => "Looking for lyrics...",
            Found::Lyrics(l) if l.synced => "Lyrics synced",
            Found::Lyrics(_) => "Plain lyrics, timing estimated",
            Found::Instrumental => "Instrumental",
            Found::NotFound => "No lyrics found",
            Found::Offline => "Could not reach LRCLIB",
        }
    }
}

#[derive(Default)]
pub struct Shared {
    pub now: Option<NowPlaying>,
    pub players: Vec<String>,
    pub found: Found,
    pub no_playerctl: bool,
    /// When `now` was last read from the player.
    pub polled: Option<Instant>,
    /// Bumped whenever `found` changes.
    pub generation: u64,
}

pub struct Paths {
    pub lyrics: PathBuf,
    pub cache: PathBuf,
}

pub struct Service {
    pub shared: Arc<Mutex<Shared>>,
    wanted: Arc<Mutex<String>>,
    online: Arc<AtomicBool>,
    alive_ms: Arc<AtomicU64>,
    refetch: Arc<AtomicBool>,
    started: Instant,
}

impl Service {
    pub fn start(paths: Paths) -> Service {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let wanted = Arc::new(Mutex::new("auto".to_owned()));
        let online = Arc::new(AtomicBool::new(true));
        let alive_ms = Arc::new(AtomicU64::new(0));
        let refetch = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        let service = Service {
            shared: shared.clone(),
            wanted: wanted.clone(),
            online: online.clone(),
            alive_ms: alive_ms.clone(),
            refetch: refetch.clone(),
            started,
        };
        let (ask, asked) = mpsc::channel::<Request>();
        let found_into = shared.clone();
        std::thread::Builder::new()
            .name("sky-lyrics-fetch".into())
            .spawn(move || {
                let mut latest_id = 0;
                while let Ok(mut request) = asked.recv() {
                    // Only the newest song matters.
                    while let Ok(newer) = asked.try_recv() {
                        request = newer;
                    }
                    latest_id = latest_id.max(request.id);
                    let found = lookup(&request.song, &paths, request.online);
                    let mut s = found_into.lock().unwrap_or_else(|e| e.into_inner());
                    if request.id == latest_id && s.now.as_ref().is_some_and(|n| n.key() == request.song.key()) {
                        s.found = found;
                        s.generation += 1;
                    }
                }
            })
            .ok();
        std::thread::Builder::new()
            .name("sky-lyrics-player".into())
            .spawn(move || poll_loop(shared, wanted, online, alive_ms, refetch, started, ask))
            .ok();
        service
    }

    /// Called each frame the mod is showing: without it, polling rests.
    pub fn keep_alive(&self, wanted: &str, online: bool) {
        self.alive_ms.store(self.started.elapsed().as_millis() as u64, Ordering::Relaxed);
        self.online.store(online, Ordering::Relaxed);
        let mut w = self.wanted.lock().unwrap_or_else(|e| e.into_inner());
        if *w != wanted {
            wanted.clone_into(&mut w);
        }
    }

    /// Looks the current song up again, skipping the cached miss.
    pub fn refetch(&self) {
        self.refetch.store(true, Ordering::Relaxed);
    }
}

struct Request {
    id: u64,
    song: NowPlaying,
    online: bool,
}

fn poll_loop(
    shared: Arc<Mutex<Shared>>,
    wanted: Arc<Mutex<String>>,
    online: Arc<AtomicBool>,
    alive_ms: Arc<AtomicU64>,
    refetch: Arc<AtomicBool>,
    started: Instant,
    ask: mpsc::Sender<Request>,
) {
    let mut id = 0u64;
    let mut current = String::new();
    loop {
        let idle = started.elapsed().as_millis() as u64 > alive_ms.load(Ordering::Relaxed) + 3000;
        if idle {
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        let run = Command::new("playerctl").args(["-a", "metadata", "--format", FORMAT]).output();
        let polled = Instant::now();
        let want = wanted.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let (names, now, missing) = match run {
            Ok(out) => {
                let (names, now) = choose(&String::from_utf8_lossy(&out.stdout), &want);
                (names, now, false)
            }
            Err(_) => (Vec::new(), None, true),
        };
        let key = now.as_ref().map(NowPlaying::key).unwrap_or_default();
        let again = refetch.swap(false, Ordering::Relaxed);
        if key != current || again {
            current.clone_from(&key);
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            // Before the lookup is asked for: a cached answer can come back at once.
            s.now.clone_from(&now);
            s.found = if key.is_empty() { Found::Nothing } else { Found::Searching };
            s.generation += 1;
            drop(s);
            if let Some(song) = now.clone().filter(|_| !key.is_empty()) {
                id += 1;
                if again {
                    forget_cache(&song);
                }
                let _ = ask.send(Request { id, song, online: online.load(Ordering::Relaxed) });
            }
        }
        {
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.now = now;
            s.players = names;
            s.no_playerctl = missing;
            s.polled = Some(polled);
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
}

fn safe_name(text: &str) -> String {
    text.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { '_' } else { c }).collect()
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// Your own `.lrc` for this song: `Artist - Title.lrc` or `Title.lrc`, any case.
fn own_file(song: &NowPlaying, dir: &Path) -> Option<PathBuf> {
    let full = format!("{} - {}.lrc", safe_name(&song.artist), safe_name(&song.title)).to_lowercase();
    let short = format!("{}.lrc", safe_name(&song.title)).to_lowercase();
    let mut fallback = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name == full {
            return Some(entry.path());
        }
        if name == short {
            fallback = Some(entry.path());
        }
    }
    fallback
}

static CACHE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

fn cache_paths(song: &NowPlaying, dir: &Path) -> (PathBuf, PathBuf) {
    let name = format!("{:016x}", fnv(&song.key()));
    (dir.join(format!("{name}.lrc")), dir.join(format!("{name}.none")))
}

fn forget_cache(song: &NowPlaying) {
    let dir = CACHE_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(dir) = dir {
        let (hit, miss) = cache_paths(song, &dir);
        let _ = std::fs::remove_file(hit);
        let _ = std::fs::remove_file(miss);
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// How long a song LRCLIB did not have stays unasked.
const MISS_SECONDS: u64 = 12 * 3600;

fn lookup(song: &NowPlaying, paths: &Paths, online: bool) -> Found {
    *CACHE_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(paths.cache.clone());
    let _ = std::fs::create_dir_all(&paths.lyrics);
    let _ = std::fs::create_dir_all(&paths.cache);
    if let Some(path) = own_file(song, &paths.lyrics) {
        if let Ok(text) = std::fs::read_to_string(path) {
            return from_text(&text, song.length_ms);
        }
    }
    let (hit, miss) = cache_paths(song, &paths.cache);
    if let Ok(text) = std::fs::read_to_string(&hit) {
        return from_text(&text, song.length_ms);
    }
    if !online {
        return Found::NotFound;
    }
    if let Ok(when) = std::fs::read_to_string(&miss) {
        if when.trim().parse::<u64>().is_ok_and(|t| unix_now().saturating_sub(t) < MISS_SECONDS) {
            return Found::NotFound;
        }
    }
    match fetch(song) {
        Ok(Some(Fetched::Text(text))) => {
            let _ = std::fs::write(&hit, &text);
            from_text(&text, song.length_ms)
        }
        Ok(Some(Fetched::Instrumental)) => Found::Instrumental,
        Ok(None) => {
            let _ = std::fs::write(&miss, unix_now().to_string());
            Found::NotFound
        }
        Err(()) => Found::Offline,
    }
}

fn from_text(text: &str, length_ms: u32) -> Found {
    let synced = lrc::parse_lrc(text);
    let lyrics = if synced.lines.is_empty() { lrc::from_plain(text, length_ms) } else { synced };
    if lyrics.lines.is_empty() {
        Found::NotFound
    } else {
        Found::Lyrics(Arc::new(lyrics))
    }
}

enum Fetched {
    Text(String),
    Instrumental,
}

const API: &str = "https://lrclib.net/api";
const AGENT: &str = "sky-lyrics-horizon-mod/1.0";

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).user_agent(AGENT).build()
}

/// "Song (feat. X) - Remastered 2011" as "Song", for a looser search.
pub fn plain_title(title: &str) -> String {
    let mut out = title;
    for cut in [" - ", " (feat", " (with", " [feat", " feat."] {
        if let Some(i) = out.to_lowercase().find(cut) {
            out = &out[..i];
        }
    }
    out.trim().to_owned()
}

fn pick_text(row: &serde_json::Value) -> Option<Fetched> {
    if row["instrumental"].as_bool() == Some(true) {
        return Some(Fetched::Instrumental);
    }
    let text = |k: &str| row[k].as_str().filter(|t| !t.trim().is_empty()).map(str::to_owned);
    text("syncedLyrics").or_else(|| text("plainLyrics")).map(Fetched::Text)
}

/// Ok(None) when LRCLIB has no such song, Err when it could not be asked.
fn fetch(song: &NowPlaying) -> Result<Option<Fetched>, ()> {
    let agent = agent();
    let mut exact = agent
        .get(&format!("{API}/get"))
        .query("artist_name", &song.artist)
        .query("track_name", &song.title);
    if !song.album.is_empty() {
        exact = exact.query("album_name", &song.album);
    }
    if song.length_ms > 0 {
        exact = exact.query("duration", &(song.length_ms / 1000).to_string());
    }
    match exact.call() {
        Ok(reply) => {
            let row = reply.into_string().ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
            if let Some(found) = row.as_ref().and_then(pick_text) {
                return Ok(Some(found));
            }
        }
        Err(ureq::Error::Status(_, _)) => {}
        Err(_) => return Err(()),
    }
    let reply = agent
        .get(&format!("{API}/search"))
        .query("artist_name", &song.artist)
        .query("track_name", &plain_title(&song.title))
        .call();
    let rows: serde_json::Value = match reply {
        Ok(reply) => serde_json::from_str(&reply.into_string().map_err(|_| ())?).map_err(|_| ())?,
        Err(ureq::Error::Status(_, _)) => return Ok(None),
        Err(_) => return Err(()),
    };
    Ok(best_row(rows.as_array().map_or(&[], Vec::as_slice), song.length_ms).and_then(pick_text))
}

/// Timed lyrics first, then the closest length.
fn best_row(rows: &[serde_json::Value], length_ms: u32) -> Option<&serde_json::Value> {
    let score = |row: &serde_json::Value| {
        let synced = row["syncedLyrics"].as_str().is_some_and(|t| !t.is_empty());
        let off = (row["duration"].as_f64().unwrap_or(0.0) - f64::from(length_ms) / 1000.0).abs();
        (u8::from(!synced), (off * 10.0) as u64)
    };
    rows.iter().filter(|r| pick_text(r).is_some()).min_by_key(|r| score(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, status: &str, artist: &str, title: &str) -> String {
        format!("{name}\t{status}\t{artist}\t{title}\tAlbum\t200000000\t5000000\n")
    }

    #[test]
    fn auto_prefers_what_is_playing_and_spotify() {
        let out = row("firefox", "Playing", "A", "Web song") + &row("spotify", "Playing", "B", "App song");
        let (names, now) = choose(&out, "auto");
        assert_eq!(names, ["firefox", "spotify"]);
        let now = now.unwrap();
        assert_eq!(now.title, "App song");
        assert_eq!((now.length_ms, now.position_ms), (200_000, 5_000));
        let paused = row("spotify", "Paused", "B", "App song") + &row("firefox", "Playing", "A", "Web song");
        assert_eq!(choose(&paused, "auto").1.unwrap().title, "Web song");
    }

    #[test]
    fn a_chosen_player_is_the_only_one_followed() {
        let out = row("firefox", "Playing", "A", "Web song");
        assert_eq!(choose(&out, "spotify").1, None);
        assert_eq!(choose(&out, "firefox").1.unwrap().title, "Web song");
    }

    #[test]
    fn browser_titles_are_split() {
        let out = row("firefox", "Playing", "", "Some Band - Good Song - YouTube");
        let now = choose(&out, "auto").1.unwrap();
        assert_eq!((now.artist.as_str(), now.title.as_str()), ("Some Band", "Good Song"));
    }

    #[test]
    fn the_clock_runs_between_polls_and_snaps_on_seeks() {
        let t = Instant::now();
        let mut np = NowPlaying { title: "x".into(), position_ms: 1000, playing: true, ..Default::default() };
        let mut clock = Clock::default();
        clock.update(&np, t);
        assert_eq!(clock.now(t + Duration::from_millis(500)), 1500);
        np.position_ms = 1600;
        clock.update(&np, t + Duration::from_millis(500));
        assert_eq!(clock.now(t + Duration::from_millis(500)), 1500, "small drift is ignored");
        np.position_ms = 60_000;
        clock.update(&np, t + Duration::from_millis(600));
        assert_eq!(clock.now(t + Duration::from_millis(600)), 60_000);
    }

    #[test]
    fn the_same_poll_every_frame_never_pulls_the_clock_back() {
        let t = Instant::now();
        let np = NowPlaying { title: "x".into(), position_ms: 1000, playing: true, ..Default::default() };
        let mut clock = Clock::default();
        let mut last = 0;
        for frame in 0..900u64 {
            let now = t + Duration::from_millis(frame);
            clock.update(&np, t);
            let at = clock.now(now);
            assert!(at >= last, "went back at frame {frame}: {last} to {at}");
            last = at;
        }
        assert_eq!(last, 1899);
    }

    #[test]
    fn titles_are_loosened_for_search() {
        assert_eq!(plain_title("Song (feat. Other) - Remastered 2011"), "Song");
        assert_eq!(plain_title("Song"), "Song");
    }

    #[test]
    fn own_files_beat_the_cache_in_any_case() {
        let dir = std::env::temp_dir().join(format!("sky-lyrics-test-{}", std::process::id()));
        let lyrics = dir.join("lyrics");
        std::fs::create_dir_all(&lyrics).unwrap();
        std::fs::write(lyrics.join("the band - my song.lrc"), "[00:01.00]mine\n").unwrap();
        let paths = Paths { lyrics, cache: dir.join("cache") };
        let song = NowPlaying { artist: "The Band".into(), title: "My Song".into(), ..Default::default() };
        let Found::Lyrics(l) = lookup(&song, &paths, false) else {
            panic!("no lyrics");
        };
        assert_eq!(l.lines[0].text(), "mine");
        let other = NowPlaying { artist: "X".into(), title: "Y".into(), ..Default::default() };
        assert_eq!(lookup(&other, &paths, false), Found::NotFound);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_best_row_is_timed_then_closest() {
        let rows: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"plainLyrics":"p","duration":200},{"syncedLyrics":"[00:01.00]s","duration":260},{"syncedLyrics":"[00:01.00]t","duration":201}]"#,
        )
        .unwrap();
        assert_eq!(best_row(&rows, 200_000).unwrap()["duration"], 201);
    }
}
