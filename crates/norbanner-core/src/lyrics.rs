//! Time-synced lyrics in the LRC format (`[01:02.50]a line`), as lyric
//! services return them, and which line is being sung at a position.

use std::time::Duration;

/// One timed line. Empty text is an instrumental gap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub at: Duration,
    pub text: String,
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
                    stamps.clear();
                    rest = "";
                    break;
                }
                None => break,
            }
            rest = after[end + 1..].trim_start();
        }
        let text = rest.trim();
        for at in stamps {
            lines.push(Line {
                at,
                text: text.to_string(),
            });
        }
    }
    lines.sort_by_key(|line| line.at);
    lines
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

/// When the line after `position` starts, if there is one.
#[must_use]
pub fn next_change(lines: &[Line], position: Duration) -> Option<Duration> {
    lines
        .get(lines.partition_point(|line| line.at <= position))
        .map(|line| line.at)
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
        assert_eq!(
            lines,
            [Line {
                at: ms(10_000),
                text: "sung line".into()
            }]
        );
    }

    #[test]
    fn repeats_a_line_at_each_of_its_timestamps_and_keeps_gaps() {
        let lines = parse("[00:20.00][00:05.00]chorus\n[00:10.00]\n[00:12.00]verse");
        let texts: Vec<_> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["chorus", "", "verse", "chorus"]);
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
}
