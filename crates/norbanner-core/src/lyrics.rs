//! Time-synced lyrics in the LRC format (`[01:02.50]a line`), as lyric
//! services return them: which line and which word is being sung at a
//! position, and which of several versions of a song to trust.
//!
//! Word timings come from enhanced LRC (`[00:01.00]<00:01.00>one <00:01.40>two`)
//! when the file has them; otherwise they are estimated by spreading the
//! line's sung time over its words by length.

use std::time::Duration;

/// A word of a line: when it starts and where it ends in the text (bytes),
/// so `&text[..end]` is everything sung once it is done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub at: Duration,
    pub end: usize,
}

/// One timed line. Empty text is an instrumental gap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub at: Duration,
    pub text: String,
    pub words: Vec<Word>,
}

/// Lines in time order. Metadata tags (`[ar:…]`, `[offset:…]`) and lines
/// without a timestamp are skipped; a line with several timestamps is
/// repeated at each of them.
#[must_use]
pub fn parse(lrc: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(after) = rest.strip_prefix('[') {
            let Some(end) = after.find(']') else { break };
            match timestamp(&after[..end]) {
                Some(at) => stamps.push(at),
                // A metadata tag, not a timestamp: the whole line is metadata.
                None if stamps.is_empty() => {
                    rest = "";
                    break;
                }
                None => break,
            }
            rest = after[end + 1..].trim_start();
        }
        if stamps.is_empty() {
            continue;
        }
        let (text, markers) = strip_word_tags(rest);
        // Word tags are absolute times: they only fit a line sung once.
        let words = if stamps.len() == 1 {
            tagged_words(&text, &markers)
        } else {
            Vec::new()
        };
        for at in stamps {
            lines.push(Line {
                at,
                text: text.clone(),
                words: words.clone(),
            });
        }
    }
    lines.sort_by_key(|line| line.at);
    estimate_words(&mut lines);
    lines
}

/// The text without `<mm:ss.xx>` tags, and where each tag was (byte offset
/// in the clean text) with its time.
fn strip_word_tags(raw: &str) -> (String, Vec<(usize, Duration)>) {
    let mut text = String::with_capacity(raw.len());
    let mut markers = Vec::new();
    let mut rest = raw;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        match timestamp(&rest[open + 1..open + close]) {
            Some(at) => {
                text.push_str(&rest[..open]);
                markers.push((text.len(), at));
            }
            None => text.push_str(&rest[..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    text.push_str(rest);
    // Trimming the start moves every offset back by what was removed.
    let lead = text.len() - text.trim_start().len();
    let text = text.trim().to_string();
    let markers = markers
        .into_iter()
        .map(|(at, time)| (at.saturating_sub(lead).min(text.len()), time))
        .collect();
    (text, markers)
}

/// Byte ranges of the words of `text`.
fn word_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                ranges.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        ranges.push((s, text.len()));
    }
    ranges
}

/// Each word takes the time of the last tag at or before its start.
fn tagged_words(text: &str, markers: &[(usize, Duration)]) -> Vec<Word> {
    if markers.is_empty() {
        return Vec::new();
    }
    word_ranges(text)
        .into_iter()
        .filter_map(|(start, end)| {
            let at = markers
                .iter()
                .take_while(|(offset, _)| *offset <= start)
                .last()
                .map(|(_, at)| *at)?;
            Some(Word { at, end })
        })
        .collect()
}

/// Gives lines without word tags estimated word times: the line is sung
/// over the time until the next one, or less if that gap is long (a pause
/// after it), and each word gets a share by its length.
fn estimate_words(lines: &mut [Line]) {
    for i in 0..lines.len() {
        if !lines[i].words.is_empty() || lines[i].text.is_empty() {
            continue;
        }
        let ranges = word_ranges(&lines[i].text);
        let weight = |(s, e): (usize, usize)| lines[i].text[s..e].chars().count() as f32 + 1.0;
        let total: f32 = ranges.iter().copied().map(weight).sum();
        // About 15 characters a second, with a little room for breath.
        let natural = 0.3 + total / 15.0;
        let gap = lines
            .get(i + 1)
            .map(|next| next.at.saturating_sub(lines[i].at).as_secs_f32() * 0.92);
        let sung = gap.map_or(natural, |gap| gap.min(natural)).max(0.05);
        let start = lines[i].at;
        let mut before = 0.0;
        let words = ranges
            .iter()
            .map(|&range| {
                let at = start + Duration::from_secs_f32(sung * before / total);
                before += weight(range);
                Word { at, end: range.1 }
            })
            .collect();
        lines[i].words = words;
    }
}

/// `mm:ss`, `mm:ss.xx`, `mm:ss.xxx` or `mm:ss:xx`.
fn timestamp(tag: &str) -> Option<Duration> {
    let (minutes, rest) = tag.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;
    let (seconds, fraction) = match rest.split_once(['.', ':']) {
        Some((seconds, fraction)) => (seconds, fraction),
        None => (rest, ""),
    };
    let seconds: u64 = seconds.trim().parse().ok()?;
    if seconds >= 60 || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let millis = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u64>().ok()? * 100,
        2 => fraction.parse::<u64>().ok()? * 10,
        _ => fraction[..3].parse::<u64>().ok()?,
    };
    Some(Duration::from_millis(
        (minutes * 60 + seconds) * 1000 + millis,
    ))
}

/// The line being sung at `position`: the last one that has started.
#[must_use]
pub fn index_at(lines: &[Line], position: Duration) -> Option<usize> {
    lines
        .partition_point(|line| line.at <= position)
        .checked_sub(1)
}

/// How many words of `line` have started at `position`.
#[must_use]
pub fn words_at(line: &Line, position: Duration) -> usize {
    line.words.partition_point(|word| word.at <= position)
}

/// When the next line or word starts after `position`, if anything does.
#[must_use]
pub fn next_change(lines: &[Line], position: Duration) -> Option<Duration> {
    let next = lines.partition_point(|line| line.at <= position);
    let next_line = lines.get(next).map(|line| line.at);
    let next_word = next
        .checked_sub(1)
        .and_then(|current| lines[current].words.iter().find(|w| w.at > position))
        .map(|word| word.at);
    match (next_line, next_word) {
        (Some(line), Some(word)) => Some(line.min(word)),
        (line, word) => line.or(word),
    }
}

/// A version of a song's lyrics a service offers.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// Its recording's length in seconds, if given.
    pub duration: Option<f64>,
    pub album: String,
    /// When its first line starts.
    pub first: Duration,
}

/// Which version to use. Volunteers time the same recording slightly
/// differently, so: keep those as long as the song, group the ones whose
/// first line starts within a moment of each other, and take the biggest
/// group (most people agree). Ties go to the album's version, then to the
/// closest length.
#[must_use]
pub fn pick(candidates: &[Candidate], length: Option<f64>, album: &str) -> Option<usize> {
    let album = album.trim().to_lowercase();
    let same_album = |c: &Candidate| {
        let theirs = c.album.trim().to_lowercase();
        !album.is_empty() && (theirs == album || theirs.starts_with(&format!("{album} (")))
    };
    let off = |c: &Candidate| match (length, c.duration) {
        (Some(length), Some(duration)) => (length - duration).abs(),
        _ => 0.0,
    };
    let mut fitting: Vec<usize> = (0..candidates.len())
        .filter(|&i| off(&candidates[i]) <= 3.0)
        .collect();
    fitting.sort_by_key(|&i| candidates[i].first);
    // Clusters of versions whose first lines start within 0.6 s.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in fitting {
        match groups.last_mut() {
            Some(group)
                if group.last().is_some_and(|&last| {
                    candidates[i].first.saturating_sub(candidates[last].first)
                        <= Duration::from_millis(600)
                }) =>
            {
                group.push(i);
            }
            _ => groups.push(vec![i]),
        }
    }
    let best_in = |group: &[usize]| {
        group.iter().copied().min_by(|&a, &b| {
            let (a, b) = (&candidates[a], &candidates[b]);
            same_album(b)
                .cmp(&same_album(a))
                .then(off(a).total_cmp(&off(b)))
        })
    };
    groups
        .iter()
        .max_by(|a, b| {
            a.len()
                .cmp(&b.len())
                .then(
                    a.iter()
                        .any(|&i| same_album(&candidates[i]))
                        .cmp(&b.iter().any(|&i| same_album(&candidates[i]))),
                )
                // The earlier group wins a full tie (max_by keeps the last).
                .then(std::cmp::Ordering::Greater)
        })
        .and_then(|group| best_in(group))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn reads_timestamps_in_every_precision() {
        let lines =
            parse("[00:01]one\n[00:02.5]two\n[00:03.25]three\n[01:04.125]four\n[00:05:50]five");
        let times: Vec<_> = lines.iter().map(|l| l.at).collect();
        assert_eq!(times, [ms(1000), ms(2500), ms(3250), ms(5500), ms(64_125)]);
        assert_eq!(lines[0].text, "one");
    }

    #[test]
    fn skips_metadata_and_untimed_lines() {
        let lines = parse("[ar:Someone]\n[ti:Something]\nplain text\n[00:10.00]  sung line  ");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].at, ms(10_000));
        assert_eq!(lines[0].text, "sung line");
    }

    #[test]
    fn repeats_a_line_at_each_of_its_timestamps_and_keeps_gaps() {
        let lines = parse("[00:20.00][00:05.00]chorus\n[00:10.00]\n[00:12.00]verse");
        let texts: Vec<_> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["chorus", "", "verse", "chorus"]);
        assert!(lines[1].words.is_empty());
    }

    #[test]
    fn finds_the_current_and_next_line() {
        let lines = parse("[00:01.00]a\n[00:03.00]b\n[00:06.00]c");
        assert_eq!(index_at(&lines, ms(500)), None);
        assert_eq!(index_at(&lines, ms(1000)), Some(0));
        assert_eq!(index_at(&lines, ms(4000)), Some(1));
        assert_eq!(index_at(&lines, ms(9000)), Some(2));
        assert_eq!(next_change(&lines, ms(500)), Some(ms(1000)));
        assert_eq!(next_change(&lines, ms(4000)), Some(ms(6000)));
        assert_eq!(next_change(&lines, ms(9000)), None);
    }

    #[test]
    fn rejects_bad_timestamps() {
        assert!(parse("[00:75.00]x\n[aa:bb]y\n[1:2.x]z").is_empty());
    }

    #[test]
    fn reads_word_tags() {
        let lines = parse("[00:01.00]<00:01.00>alpha <00:01.50>beta <00:02.25>gamma\n[00:04.00]x");
        let line = &lines[0];
        assert_eq!(line.text, "alpha beta gamma");
        let words: Vec<_> = line
            .words
            .iter()
            .map(|w| (w.at, &line.text[..w.end]))
            .collect();
        assert_eq!(
            words,
            [
                (ms(1000), "alpha"),
                (ms(1500), "alpha beta"),
                (ms(2250), "alpha beta gamma")
            ]
        );
        assert_eq!(words_at(line, ms(1600)), 2);
        assert_eq!(next_change(&lines, ms(1600)), Some(ms(2250)));
    }

    #[test]
    fn estimates_words_in_order_within_the_line() {
        let lines = parse("[00:10.00]aa bbbb cc\n[00:12.00]next");
        let words = &lines[0].words;
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].at, ms(10_000));
        assert!(words[0].at < words[1].at && words[1].at < words[2].at);
        assert!(words[2].at < ms(12_000));
        // The longer word takes longer.
        assert!(words[2].at - words[1].at > words[1].at - words[0].at);
        assert_eq!(&lines[0].text[..words[1].end], "aa bbbb");
        // A long pause after a short line does not stretch it.
        let lines = parse("[00:10.00]short\n[00:40.00]next");
        assert_eq!(lines[0].words.len(), 1);
        let lines = parse("[00:10.00]two words\n[00:40.00]next");
        assert!(lines[0].words[1].at < ms(11_000));
    }

    fn candidate(duration: f64, album: &str, first: u64) -> Candidate {
        Candidate {
            duration: Some(duration),
            album: album.into(),
            first: ms(first),
        }
    }

    #[test]
    fn picks_the_version_most_agree_on() {
        let candidates = [
            candidate(267.0, "Hits", 2040),
            candidate(269.0, "Other", 16_700),
            candidate(269.0, "Other", 16_750),
            candidate(269.0, "Album", 17_910),
            candidate(269.0, "Album (Deluxe)", 19_390),
            candidate(269.0, "Other", 19_320),
            candidate(269.0, "Other", 19_340),
            candidate(248.0, "Other", 19_300),
        ];
        let chosen = pick(&candidates, Some(269.2), "Album").map(|i| candidates[i].first);
        assert_eq!(chosen, Some(ms(19_390)));
    }

    #[test]
    fn album_breaks_ties_and_nothing_fits_gives_none() {
        let candidates = [
            candidate(200.0, "Other", 5000),
            candidate(200.0, "Album", 9000),
        ];
        assert_eq!(pick(&candidates, Some(200.0), "Album"), Some(1));
        assert_eq!(pick(&candidates, Some(100.0), "Album"), None);
        assert_eq!(pick(&[], None, ""), None);
    }
}
