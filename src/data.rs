use crate::hosts::{Entry, HostsDocument};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Hosts,
    Json,
    Yaml,
    Toml,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntryFile {
    #[serde(default = "schema_version")]
    version: u32,
    entries: Vec<Entry>,
}

pub fn serialize_entries(entries: &[Entry], format: DataFormat) -> io::Result<String> {
    match format {
        DataFormat::Hosts => {
            let mut output = String::new();
            for entry in entries {
                let mut line = format!("{}\t{}", entry.ip, entry.hostnames.join(" "));
                if let Some(comment) = &entry.comment {
                    line.push_str(&format!("\t# {comment}"));
                }
                if entry.disabled {
                    line = format!("# hostctl-disabled: {line}");
                }
                output.push_str(&line);
                output.push('\n');
            }
            Ok(output)
        }
        DataFormat::Json => serde_json::to_string_pretty(&EntryFile {
            version: schema_version(),
            entries: entries.to_vec(),
        })
        .map_err(serialization_error),
        DataFormat::Yaml => serde_yaml::to_string(&EntryFile {
            version: schema_version(),
            entries: entries.to_vec(),
        })
        .map_err(serialization_error),
        DataFormat::Toml => toml::to_string_pretty(&EntryFile {
            version: schema_version(),
            entries: entries.to_vec(),
        })
        .map_err(serialization_error),
    }
}

pub fn parse_entries(content: &str, format: DataFormat) -> io::Result<Vec<Entry>> {
    let file = match format {
        DataFormat::Hosts => {
            return Ok(HostsDocument::parse(content).all_entries());
        }
        DataFormat::Json => serde_json::from_str::<EntryFile>(content).map_err(parse_error)?,
        DataFormat::Yaml => serde_yaml::from_str::<EntryFile>(content).map_err(parse_error)?,
        DataFormat::Toml => toml::from_str::<EntryFile>(content).map_err(parse_error)?,
    };
    if file.version != schema_version() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported data version {}; expected 1", file.version),
        ));
    }
    validate_entries(&file.entries)?;
    Ok(file.entries)
}

pub fn validate_entries(entries: &[Entry]) -> io::Result<()> {
    let mut document = HostsDocument::parse("");
    document.replace_active(entries)
}

fn schema_version() -> u32 {
    1
}

fn serialization_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("cannot serialize entries: {error}"))
}

fn parse_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("cannot parse entries: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        vec![Entry {
            ip: "127.0.0.1".to_string(),
            hostnames: vec!["app.local".to_string(), "api.local".to_string()],
            comment: Some("development".to_string()),
            disabled: false,
        }]
    }

    #[test]
    fn structured_formats_round_trip() {
        for format in [DataFormat::Json, DataFormat::Yaml, DataFormat::Toml] {
            let serialized = serialize_entries(&entries(), format).unwrap();
            assert_eq!(parse_entries(&serialized, format).unwrap(), entries());
        }
    }
}
