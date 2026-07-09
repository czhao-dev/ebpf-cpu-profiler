//! JIT symbol resolution: parses `/tmp/perf-<pid>.map`, the line-based
//! address-range-to-symbol format written by JIT engines running with
//! basic profiling enabled (Node's `--perf-basic-prof`, perf-map-agent
//! for the JVM, etc.), so JIT-compiled code - which lives in anonymous
//! mappings invisible to `/proc/<pid>/maps` + ELF symbol table lookups -
//! can still be labeled instead of falling back to `[unknown]`.

use std::path::PathBuf;

/// Sorted `(start, end, name)` ranges parsed from a perf map file.
/// Addresses are absolute process virtual addresses (unlike
/// `BinarySymbolTable` in `usersym.rs`, which stores file-relative
/// addresses resolved through `/proc/<pid>/maps`), since that's what
/// JIT engines write to the map file.
pub struct JitSymbolTable {
    entries: Vec<(u64, u64, String)>,
}

impl JitSymbolTable {
    /// Reads `/tmp/perf-<pid>.map`. A missing file (the common case - most
    /// processes have no JIT engine writing one) yields an empty table
    /// rather than an error.
    pub fn load(pid: u32) -> anyhow::Result<Self> {
        let path = PathBuf::from(format!("/tmp/perf-{pid}.map"));
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self::parse(&text))
    }

    pub(crate) fn parse(text: &str) -> Self {
        let mut entries: Vec<(u64, u64, String)> =
            text.lines().filter_map(parse_map_line).collect();
        entries.sort_by_key(|(start, _, _)| *start);
        Self { entries }
    }

    /// The symbol whose `[start, start+size)` range contains `ip`, as
    /// `(name, offset)`.
    pub fn resolve(&self, ip: u64) -> Option<(&str, u64)> {
        let idx = self.entries.partition_point(|(start, _, _)| *start <= ip);
        if idx == 0 {
            return None;
        }
        let (start, end, name) = &self.entries[idx - 1];
        (ip < *end).then(|| (name.as_str(), ip - start))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Parses one `HEX_START HEX_SIZE symbol_name` line. The symbol name is
/// the remainder of the line after the first two whitespace-separated
/// tokens, since it may itself contain spaces (e.g. V8's
/// `LazyCompile:*foo /path/to/file.js:10:5`).
fn parse_map_line(line: &str) -> Option<(u64, u64, String)> {
    let tokens: Vec<&str> = line.splitn(3, char::is_whitespace).collect();
    if tokens.len() < 3 {
        return None;
    }
    let start = u64::from_str_radix(tokens[0], 16).ok()?;
    let size = u64::from_str_radix(tokens[1], 16).ok()?;
    let name = tokens[2].trim_start();
    if name.is_empty() {
        return None;
    }
    Some((start, start + size, name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP_FIXTURE: &str = "\
7f0000000000 40 LazyCompile:*foo /tmp/x.js:1:1
7f0000001000 100 LazyCompile:*bar /tmp/x.js:5:1
";

    #[test]
    fn resolves_addresses_within_range() {
        let table = JitSymbolTable::parse(MAP_FIXTURE);
        assert_eq!(table.resolve(0x7f0000000000), Some(("LazyCompile:*foo /tmp/x.js:1:1", 0)));
        assert_eq!(table.resolve(0x7f0000000010), Some(("LazyCompile:*foo /tmp/x.js:1:1", 0x10)));
        assert_eq!(table.resolve(0x7f0000001050), Some(("LazyCompile:*bar /tmp/x.js:5:1", 0x50)));
    }

    #[test]
    fn addresses_outside_any_range_are_unresolved() {
        let table = JitSymbolTable::parse(MAP_FIXTURE);
        assert_eq!(table.resolve(0x7f0000000040), None); // exactly at foo's end (exclusive)
        assert_eq!(table.resolve(0x6f0000000000), None); // before the first entry
        assert_eq!(table.resolve(0x7f0000000900), None); // gap between foo and bar
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let text = "\
not-hex 40 bad_start
7f0000000000 not-hex bad_size
7f0000000000 40
\n\
7f0000002000 10 ok_entry
";
        let table = JitSymbolTable::parse(text);
        assert_eq!(table.resolve(0x7f0000002000), Some(("ok_entry", 0)));
        assert_eq!(table.resolve(0x0), None);
    }

    #[test]
    fn empty_file_yields_empty_table() {
        let table = JitSymbolTable::parse("");
        assert!(table.is_empty());
        assert_eq!(table.resolve(0x1000), None);
    }

    #[test]
    fn later_entry_wins_on_duplicate_start() {
        let text = "\
1000 10 first
1000 20 second
";
        let table = JitSymbolTable::parse(text);
        assert_eq!(table.resolve(0x1000), Some(("second", 0)));
    }

    #[test]
    fn missing_map_file_yields_empty_table() {
        // No JIT process actually runs under this pid during tests.
        let table = JitSymbolTable::load(1).unwrap();
        assert!(table.is_empty());
    }
}
