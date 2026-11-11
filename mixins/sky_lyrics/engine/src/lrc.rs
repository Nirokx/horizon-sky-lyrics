//! Lyric timing: LRC files (with or without word tags) and plain text, as
//! lines of words, every time in milliseconds of the song.

#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub start: u32,
    /// When the singing stops: the line may stay up longer, up to the next.
    pub end: u32,
    pub words: Vec<Word>,
}

impl Line {
    pub fn text(&self) -> String {
        self.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    /// False when the times are only spread over the song.
    pub synced: bool,
}

impl Lyrics {
    /// The line up at `at`: the last one started, if any.
    pub fn index_at(&self, at: u32) -> Option<usize> {
        self.lines.partition_point(|l| l.start <= at).checked_sub(1)
    }

    pub fn text(&self) -> String {
        self.lines.iter().map(Line::text).collect::<Vec<_>>().join("\n")
    }
}

/// `mm:ss.xx` (or `mm:ss:xx`, `mm:ss`) in milliseconds.
fn parse_time(tag: &str) -> Option<u32> {
    let (minutes, rest) = tag.split_once(':')?;
    let minutes: u32 = minutes.trim().parse().ok()?;
    let rest = rest.replace(':', ".");
    let (seconds, fraction) = rest.split_once('.').unwrap_or((rest.as_str(), ""));
    let seconds: u32 = seconds.trim().parse().ok()?;
    let digits: String = fraction.chars().take_while(char::is_ascii_digit).take(3).collect();
    let ms = if digits.is_empty() {
        0
    } else {
        digits.parse::<u32>().ok()? * 10u32.pow(3 - digits.len() as u32)
    };
    Some(minutes * 60_000 + seconds * 1000 + ms)
}

/// How long singing `text` probably takes.
fn sung_length(text: &str) -> u32 {
    (text.chars().count() as u32 * 75 + 900).min(9000)
}

/// Words of `text` spread over `start..end`, by their length.
fn spread_words(text: &str, start: u32, end: u32) -> Vec<Word> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let total: u32 = parts.iter().map(|w| w.chars().count() as u32 + 1).sum();
    let span = u64::from(end.saturating_sub(start));
    let mut used = 0u32;
    parts
        .iter()
        .map(|word| {
            let from = start + (u64::from(used) * span / u64::from(total.max(1))) as u32;
            used += word.chars().count() as u32 + 1;
            let to = start + (u64::from(used) * span / u64::from(total.max(1))) as u32;
            Word { start: from, end: to, text: (*word).to_owned() }
        })
        .collect()
}

/// A line's `<mm:ss.xx>` word tags, as (start, words) pairs.
fn tagged_words(text: &str) -> Option<Vec<(u32, String)>> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    if !rest.starts_with('<') {
        return None;
    }
    while let Some(open) = rest.strip_prefix('<') {
        let close = open.find('>')?;
        let time = parse_time(&open[..close])?;
        let after = &open[close + 1..];
        let end = after.find('<').unwrap_or(after.len());
        let chunk = after[..end].trim();
        if !chunk.is_empty() {
            out.push((time, chunk.to_owned()));
        }
        rest = &after[end..];
    }
    (!out.is_empty()).then_some(out)
}

/// A parsed `.lrc`: lines in order, an empty stamped line ending the one before.
pub fn parse_lrc(text: &str) -> Lyrics {
    let mut stamped: Vec<(u32, String)> = Vec::new();
    let mut offset = 0i64;
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(body) = rest.strip_prefix('[') {
            let Some(close) = body.find(']') else {
                break;
            };
            let tag = &body[..close];
            if let Some(value) = tag.strip_prefix("offset:") {
                offset = value.trim().parse().unwrap_or(0);
            } else if let Some(time) = parse_time(tag) {
                times.push(time);
            }
            rest = body[close + 1..].trim_start();
        }
        for time in times {
            stamped.push((time, rest.to_owned()));
        }
    }
    stamped.sort_by_key(|(t, _)| *t);
    let shifted = |t: u32| (i64::from(t) + offset).max(0) as u32;
    let mut lines = Vec::new();
    for (i, (time, body)) in stamped.iter().enumerate() {
        if body.trim().is_empty() {
            continue;
        }
        let next = stamped.get(i + 1).map(|(t, _)| *t);
        let start = shifted(*time);
        let words: Vec<Word> = match tagged_words(body) {
            Some(tagged) => tagged
                .iter()
                .enumerate()
                .flat_map(|(j, (from, chunk))| {
                    let to = tagged.get(j + 1).map(|(t, _)| *t).or(next).unwrap_or(from + sung_length(chunk));
                    spread_words(chunk, shifted(*from), shifted(to))
                })
                .collect(),
            None => {
                let sung = sung_length(body);
                let end = next.map_or(*time + sung, |n| n.min(*time + sung));
                spread_words(body, start, shifted(end))
            }
        };
        let end = next.map_or(start + sung_length(body), shifted).min(words.last().map_or(start, |w| w.end));
        if !words.is_empty() {
            lines.push(Line { start, end, words });
        }
    }
    Lyrics { synced: !lines.is_empty(), lines }
}

/// Plain lyrics with no times, spread over the song.
pub fn from_plain(text: &str, length_ms: u32) -> Lyrics {
    let rows: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if rows.is_empty() {
        return Lyrics::default();
    }
    let weights: Vec<u32> = rows.iter().map(|r| sung_length(r)).collect();
    let total: u64 = weights.iter().map(|w| u64::from(*w)).sum();
    // Most songs open and close without singing.
    let length = u64::from(if length_ms == 0 { (total as u32).saturating_add(10_000) } else { length_ms });
    let (margin, span) = (length / 12, length - length / 6);
    let mut used = 0u64;
    let mut lines = Vec::new();
    for (row, weight) in rows.iter().zip(&weights) {
        let start = (margin + used * span / total) as u32;
        used += u64::from(*weight);
        let next = (margin + used * span / total) as u32;
        let end = next.min(start + weight);
        lines.push(Line { start, end, words: spread_words(row, start, end) });
    }
    Lyrics { lines, synced: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_in_every_form() {
        assert_eq!(parse_time("01:02.50"), Some(62_500));
        assert_eq!(parse_time("00:03.123"), Some(3_123));
        assert_eq!(parse_time("00:03:50"), Some(3_500));
        assert_eq!(parse_time("10:00"), Some(600_000));
        assert_eq!(parse_time("ar"), None);
    }

    #[test]
    fn lines_sort_repeat_and_end_on_blank_stamps() {
        let l = parse_lrc("[ar:Someone]\n[00:10.00][00:30.00]again and again\n[00:05.00]first words\n[00:12.00]\n");
        assert_eq!(l.lines.iter().map(|x| x.start).collect::<Vec<_>>(), [5_000, 10_000, 30_000]);
        assert_eq!(l.lines[1].text(), "again and again");
        assert!(l.lines[1].end <= 12_000);
        assert!(l.synced);
    }

    #[test]
    fn words_are_spread_inside_the_line_in_order() {
        let l = parse_lrc("[00:01.00]one two three\n[00:04.00]next\n");
        let words = &l.lines[0].words;
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].start, 1_000);
        assert!(words.windows(2).all(|w| w[0].end == w[1].start));
        assert!(words[2].end <= 4_000);
    }

    #[test]
    fn word_tags_give_exact_times() {
        let l = parse_lrc("[00:01.00]<00:01.00>hello <00:01.80>there\n[00:03.00]x\n");
        assert_eq!(l.lines[0].words[1].start, 1_800);
        assert_eq!(l.lines[0].text(), "hello there");
    }

    #[test]
    fn the_offset_tag_moves_every_line() {
        let l = parse_lrc("[offset:500]\n[00:01.00]late\n");
        assert_eq!(l.lines[0].start, 1_500);
    }

    #[test]
    fn plain_text_stays_inside_the_song() {
        let l = from_plain("one line\n\nanother line\nand a third", 180_000);
        assert_eq!(l.lines.len(), 3);
        assert!(!l.synced);
        assert!(l.lines.windows(2).all(|w| w[0].start < w[1].start && w[0].end <= w[1].start));
        assert!(l.lines[2].end < 180_000);
    }

    #[test]
    fn the_line_up_is_the_last_started() {
        let l = parse_lrc("[00:02.00]a\n[00:04.00]b\n");
        assert_eq!(l.index_at(1_000), None);
        assert_eq!(l.index_at(2_000), Some(0));
        assert_eq!(l.index_at(99_000), Some(1));
    }
}
