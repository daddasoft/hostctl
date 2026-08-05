use std::{io, net::IpAddr, ops::Range};

#[derive(Debug, Clone)]
struct Line {
    body: String,
    ending: String,
}

#[derive(Debug)]
struct ParsedEntry {
    ip: Range<usize>,
    hosts: Vec<Range<usize>>,
    comment_start: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub ip: String,
    pub hostnames: Vec<String>,
}

#[derive(Debug)]
pub struct HostsDocument {
    bom: bool,
    lines: Vec<Line>,
    preferred_ending: String,
    had_final_ending: bool,
}

impl HostsDocument {
    pub fn parse(content: &str) -> Self {
        let (bom, content) = match content.strip_prefix('\u{feff}') {
            Some(rest) => (true, rest),
            None => (false, content),
        };
        let mut lines = Vec::new();
        let mut start = 0;

        for (index, _) in content.match_indices('\n') {
            let (body_end, ending) = if index > start && content.as_bytes()[index - 1] == b'\r' {
                (index - 1, "\r\n")
            } else {
                (index, "\n")
            };
            lines.push(Line {
                body: content[start..body_end].to_string(),
                ending: ending.to_string(),
            });
            start = index + 1;
        }
        if start < content.len() {
            lines.push(Line {
                body: content[start..].to_string(),
                ending: String::new(),
            });
        }

        let preferred_ending = lines
            .iter()
            .find(|line| !line.ending.is_empty())
            .map(|line| line.ending.clone())
            .unwrap_or_else(default_line_ending);
        let had_final_ending = lines.last().is_some_and(|line| !line.ending.is_empty());

        Self {
            bom,
            lines,
            preferred_ending,
            had_final_ending,
        }
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        if self.bom {
            output.push('\u{feff}');
        }
        for line in &self.lines {
            output.push_str(&line.body);
            output.push_str(&line.ending);
        }
        output
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.lines
            .iter()
            .filter_map(|line| {
                let entry = parse_entry(&line.body)?;
                Some(Entry {
                    ip: line.body[entry.ip].to_string(),
                    hostnames: entry
                        .hosts
                        .iter()
                        .map(|span| line.body[span.clone()].to_string())
                        .collect(),
                })
            })
            .collect()
    }

    pub fn add(
        &mut self,
        ip: &str,
        hostname: &str,
        comment: Option<&str>,
        force: bool,
        overwrite: bool,
    ) -> io::Result<()> {
        validate_add_input(ip, hostname, comment, force, overwrite)?;

        let duplicate = self.lines.iter().any(|line| line_has_host(line, hostname));
        if duplicate && !force && !overwrite {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "hostname '{hostname}' already exists. Use --force to add another entry or --overwrite to replace it"
                ),
            ));
        }

        if duplicate && overwrite {
            self.overwrite(ip, hostname, comment);
        } else {
            self.append_line(format_entry(ip, hostname, comment));
        }
        self.restore_final_ending();
        Ok(())
    }

    pub fn remove(&mut self, hostname: &str) -> io::Result<usize> {
        validate_hostname(hostname)?;
        let mut removed = 0;
        let mut output = Vec::with_capacity(self.lines.len());

        for mut line in self.lines.drain(..) {
            let Some(entry) = parse_entry(&line.body) else {
                output.push(line);
                continue;
            };
            let matching = entry
                .hosts
                .iter()
                .filter(|span| line.body[(*span).clone()].eq_ignore_ascii_case(hostname))
                .count();
            if matching == 0 {
                output.push(line);
                continue;
            }

            removed += matching;
            if matching < entry.hosts.len() {
                line.body = remove_host_tokens(&line.body, &entry, hostname);
                output.push(line);
            }
        }

        if removed == 0 {
            self.lines = output;
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("hostname '{hostname}' not found in hosts file"),
            ));
        }
        self.lines = output;
        self.restore_final_ending();
        Ok(removed)
    }

    fn overwrite(&mut self, ip: &str, hostname: &str, comment: Option<&str>) {
        let mut output = Vec::with_capacity(self.lines.len());

        for mut line in self.lines.drain(..) {
            let Some(entry) = parse_entry(&line.body) else {
                output.push(line);
                continue;
            };
            let matching = entry
                .hosts
                .iter()
                .filter(|span| line.body[(*span).clone()].eq_ignore_ascii_case(hostname))
                .count();
            if matching == 0 {
                output.push(line);
                continue;
            }

            if matching == entry.hosts.len() {
                line.body.replace_range(entry.ip, ip);
                if let Some(comment) = comment {
                    line.body = replace_comment(&line.body, comment);
                }
                output.push(line);
                continue;
            }

            line.body = remove_host_tokens(&line.body, &entry, hostname);
            let original_ending = line.ending.clone();
            if line.ending.is_empty() {
                line.ending = self.preferred_ending.clone();
            }
            output.push(line);
            output.push(Line {
                body: format_entry(ip, hostname, comment),
                ending: original_ending,
            });
        }
        self.lines = output;
    }

    fn append_line(&mut self, body: String) {
        let was_empty = self.lines.is_empty();
        if let Some(last) = self.lines.last_mut()
            && last.ending.is_empty()
        {
            last.ending = self.preferred_ending.clone();
        }
        self.lines.push(Line {
            body,
            ending: if was_empty || self.had_final_ending {
                self.preferred_ending.clone()
            } else {
                String::new()
            },
        });
    }

    fn restore_final_ending(&mut self) {
        let Some(last) = self.lines.last_mut() else {
            return;
        };
        if self.had_final_ending {
            if last.ending.is_empty() {
                last.ending = self.preferred_ending.clone();
            }
        } else {
            last.ending.clear();
        }
    }
}

pub fn validate_hostname(hostname: &str) -> io::Result<()> {
    if hostname.is_empty() || hostname.len() > 254 || !hostname.is_ascii() {
        return invalid("hostname must be ASCII and no longer than 254 bytes");
    }
    if hostname.chars().any(char::is_control) || hostname.contains(char::is_whitespace) {
        return invalid("hostname must not contain whitespace or control characters");
    }

    let normalized = hostname.strip_suffix('.').unwrap_or(hostname);
    if normalized.is_empty() {
        return invalid("hostname must contain at least one label");
    }
    for label in normalized.split('.') {
        if label.is_empty() || label.len() > 63 {
            return invalid("each hostname label must contain between 1 and 63 characters");
        }
        if !label
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
            || !label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return invalid(
                "hostname labels must start and end with a letter or digit and may contain hyphens",
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_add_input(
    ip: &str,
    hostname: &str,
    comment: Option<&str>,
    force: bool,
    overwrite: bool,
) -> io::Result<()> {
    if force && overwrite {
        return invalid("--force and --overwrite cannot be used together");
    }
    validate_ip(ip)?;
    validate_hostname(hostname)?;
    validate_comment(comment)
}

fn validate_ip(ip: &str) -> io::Result<()> {
    ip.parse::<IpAddr>().map(|_| ()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{ip}' is not a valid IP address"),
        )
    })
}

fn validate_comment(comment: Option<&str>) -> io::Result<()> {
    if comment.is_some_and(|value| value.chars().any(char::is_control)) {
        return invalid("comment must not contain control characters");
    }
    Ok(())
}

fn parse_entry(body: &str) -> Option<ParsedEntry> {
    let trimmed = body.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let comment_start = body.find('#');
    let data = &body[..comment_start.unwrap_or(body.len())];
    let tokens = token_spans(data);
    if tokens.len() < 2 || data[tokens[0].clone()].parse::<IpAddr>().is_err() {
        return None;
    }
    Some(ParsedEntry {
        ip: tokens[0].clone(),
        hosts: tokens[1..].to_vec(),
        comment_start,
    })
}

fn token_spans(value: &str) -> Vec<Range<usize>> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, character) in value.char_indices() {
        if character.is_whitespace() {
            if let Some(token_start) = start.take() {
                tokens.push(token_start..index);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(token_start) = start {
        tokens.push(token_start..value.len());
    }
    tokens
}

fn line_has_host(line: &Line, hostname: &str) -> bool {
    parse_entry(&line.body).is_some_and(|entry| {
        entry
            .hosts
            .iter()
            .any(|span| line.body[span.clone()].eq_ignore_ascii_case(hostname))
    })
}

fn remove_host_tokens(body: &str, entry: &ParsedEntry, hostname: &str) -> String {
    let mut output = String::with_capacity(body.len());
    output.push_str(&body[..entry.ip.end]);
    let mut previous_end = entry.ip.end;
    for span in &entry.hosts {
        if !body[span.clone()].eq_ignore_ascii_case(hostname) {
            output.push_str(&body[previous_end..span.start]);
            output.push_str(&body[span.clone()]);
        }
        previous_end = span.end;
    }
    output.push_str(&body[previous_end..]);
    output
}

fn replace_comment(body: &str, comment: &str) -> String {
    let entry = parse_entry(body).expect("entry was parsed before comment replacement");
    let data_end = entry.comment_start.unwrap_or(body.len());
    format!("{}\t# {comment}", body[..data_end].trim_end())
}

fn format_entry(ip: &str, hostname: &str, comment: Option<&str>) -> String {
    match comment {
        Some(comment) => format!("{ip}\t{hostname}\t# {comment}"),
        None => format!("{ip}\t{hostname}"),
    }
}

fn invalid<T>(message: impl Into<String>) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

fn default_line_ending() -> String {
    if cfg!(windows) { "\r\n" } else { "\n" }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_original_bytes() {
        let input = "\u{feff}# heading\r\n127.0.0.1   localhost alias\t# note\r\n\r\n";
        assert_eq!(HostsDocument::parse(input).render(), input);
    }

    #[test]
    fn remove_keeps_other_alias_and_formatting() {
        let input = "127.0.0.1   localhost\tapp.local  # note\r\n";
        let mut document = HostsDocument::parse(input);
        assert_eq!(document.remove("app.local").unwrap(), 1);
        assert_eq!(document.render(), "127.0.0.1   localhost  # note\r\n");
    }

    #[test]
    fn overwrite_splits_multi_alias_line() {
        let mut document = HostsDocument::parse("10.0.0.1 app.local api.local\n");
        document
            .add("10.0.0.2", "app.local", Some("new"), false, true)
            .unwrap();
        assert_eq!(
            document.render(),
            "10.0.0.1 api.local\n10.0.0.2\tapp.local\t# new\n"
        );
    }

    #[test]
    fn mutation_preserves_missing_final_newline() {
        let mut document = HostsDocument::parse("127.0.0.1 localhost");
        document
            .add("10.0.0.1", "app.local", None, false, false)
            .unwrap();
        let ending = if cfg!(windows) { "\r\n" } else { "\n" };
        assert_eq!(
            document.render(),
            format!("127.0.0.1 localhost{ending}10.0.0.1\tapp.local")
        );
    }

    #[test]
    fn validation_rejects_invalid_hostnames_and_comments() {
        assert!(validate_hostname("-bad.local").is_err());
        assert!(validate_hostname("bad..local").is_err());
        assert!(validate_hostname("bad_name.local").is_err());
        assert!(validate_comment(Some("bad\ncomment")).is_err());
    }
}
