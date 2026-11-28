//! The licence check (R6). Every dependency must be usable under a licence
//! that is not GPL, LGPL, or AGPL.
//!
//! Also keeps the list of third party components bundled into the program,
//! shown in About Brick (F16), up to date.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

pub fn run(check: bool) -> Result<(), String> {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .map_err(|e| format!("failed to run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("cargo metadata returned invalid JSON: {e}"))?;

    let packages = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata has no package list")?;

    // licence expression -> packages using it
    let mut by_licence: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut violations = Vec::new();

    for package in packages {
        let name = package["name"].as_str().unwrap_or("?");
        let version = package["version"].as_str().unwrap_or("?");
        let id = format!("{name} {version}");
        let licence = package["license"].as_str();

        match licence.map(allowed) {
            Some(Ok(true)) => {}
            Some(Ok(false)) => {
                violations.push(format!("{id}: {}", licence.unwrap_or_default()));
            }
            Some(Err(e)) => violations.push(format!("{id}: unreadable licence expression ({e})")),
            None => violations.push(format!("{id}: no SPDX licence declared, review manually")),
        }
        by_licence
            .entry(licence.unwrap_or("<none>").to_owned())
            .or_default()
            .push(id);
    }

    if !check {
        for (licence, ids) in &by_licence {
            println!("{licence} ({})", ids.len());
            for id in ids {
                println!("    {id}");
            }
        }
    }

    let bundled = bundled_list(&metadata)?;
    let bundled_path = bundled_path();
    let current = std::fs::read_to_string(&bundled_path).unwrap_or_default();
    if current != bundled {
        if check {
            violations.push(format!(
                "{} is out of date, run `cargo xtask licences`",
                bundled_path.display()
            ));
        } else {
            std::fs::write(&bundled_path, &bundled)
                .map_err(|e| format!("failed to write {}: {e}", bundled_path.display()))?;
            println!("wrote {}", bundled_path.display());
        }
    }

    if violations.is_empty() {
        if check {
            println!("licence check passed: {} packages", packages.len());
        }
        Ok(())
    } else {
        let list = violations.join("\n    ");
        let message = format!("licence check failed:\n    {list}");
        if check {
            Err(message)
        } else {
            eprintln!("warning: {message}");
            Ok(())
        }
    }
}

/// Where the list of bundled components is written. The program includes it
/// at compile time.
fn bundled_path() -> PathBuf {
    crate::edsdk::workspace_root().join("crates/brick/third_party_licences.tsv")
}

/// The third party packages the program is built from on any platform, one
/// `name<TAB>version<TAB>licence` line each, sorted. Build and dev
/// dependencies are not part of the program and are left out.
fn bundled_list(metadata: &Value) -> Result<String, String> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata has no package list")?;
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .ok_or("cargo metadata has no dependency graph")?;
    let members: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .ok_or("cargo metadata has no workspace members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let program = packages
        .iter()
        .find(|p| p["name"] == "brick" && members.contains(p["id"].as_str().unwrap_or_default()))
        .and_then(|p| p["id"].as_str())
        .ok_or("the workspace has no brick package")?;

    let mut seen = BTreeSet::from([program]);
    let mut stack = vec![program];
    while let Some(id) = stack.pop() {
        let node = nodes
            .iter()
            .find(|n| n["id"] == id)
            .ok_or_else(|| format!("{id} is missing from the dependency graph"))?;
        for dep in node["deps"].as_array().into_iter().flatten() {
            let normal = dep["dep_kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|kind| kind["kind"].is_null());
            if let Some(dep_id) = dep["pkg"].as_str()
                && normal
                && seen.insert(dep_id)
            {
                stack.push(dep_id);
            }
        }
    }

    let mut lines: Vec<String> = packages
        .iter()
        .filter(|p| {
            let id = p["id"].as_str().unwrap_or_default();
            seen.contains(id) && !members.contains(id)
        })
        .map(|p| {
            format!(
                "{}\t{}\t{}",
                p["name"].as_str().unwrap_or("?"),
                p["version"].as_str().unwrap_or("?"),
                p["license"].as_str().unwrap_or("unknown"),
            )
        })
        .collect();
    lines.sort();
    lines.dedup();
    let mut list = lines.join("\n");
    list.push('\n');
    Ok(list)
}

/// Whether an SPDX licence expression can be satisfied without any
/// GPL-family licence.
fn allowed(expression: &str) -> Result<bool, String> {
    // Older crates use `/` as a shorthand for OR.
    let expression = expression.replace('/', " OR ");
    let tokens = tokenize(&expression);
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
    };
    let result = parser.or_expr()?;
    if parser.pos != tokens.len() {
        return Err(format!("unexpected `{}`", tokens[parser.pos]));
    }
    Ok(result)
}

fn tokenize(expression: &str) -> Vec<String> {
    expression
        .replace('(', " ( ")
        .replace(')', " ) ")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Recursive descent over `or := and (OR and)*`, `and := term (AND term)*`,
/// `term := '(' or ')' | id [WITH id]`, evaluating whether each branch is
/// free of GPL-family licences.
struct Parser<'a> {
    tokens: &'a [String],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.pos).map(String::as_str)
    }

    fn next(&mut self) -> Result<&str, String> {
        let token = self.tokens.get(self.pos).ok_or("unexpected end")?;
        self.pos += 1;
        Ok(token)
    }

    fn or_expr(&mut self) -> Result<bool, String> {
        let mut ok = self.and_expr()?;
        while self.peek() == Some("OR") {
            self.pos += 1;
            ok |= self.and_expr()?;
        }
        Ok(ok)
    }

    fn and_expr(&mut self) -> Result<bool, String> {
        let mut ok = self.term()?;
        while self.peek() == Some("AND") {
            self.pos += 1;
            ok &= self.term()?;
        }
        Ok(ok)
    }

    fn term(&mut self) -> Result<bool, String> {
        if self.peek() == Some("(") {
            self.pos += 1;
            let ok = self.or_expr()?;
            if self.next()? != ")" {
                return Err("missing `)`".into());
            }
            return Ok(ok);
        }
        let id = self.next()?.to_owned();
        if self.peek() == Some("WITH") {
            self.pos += 2;
        }
        Ok(!id.to_ascii_uppercase().contains("GPL"))
    }
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn permissive_licences_pass() {
        assert_eq!(allowed("MIT"), Ok(true));
        assert_eq!(allowed("MIT OR Apache-2.0"), Ok(true));
        assert_eq!(allowed("MIT/Apache-2.0"), Ok(true));
        assert_eq!(allowed("(MIT OR Apache-2.0) AND Unicode-3.0"), Ok(true));
        assert_eq!(allowed("Apache-2.0 WITH LLVM-exception"), Ok(true));
    }

    #[test]
    fn gpl_alternative_is_not_required() {
        assert_eq!(allowed("MIT OR GPL-3.0-or-later"), Ok(true));
        assert_eq!(allowed("LGPL-2.1-only OR MPL-2.0"), Ok(true));
    }

    #[test]
    fn gpl_family_fails() {
        assert_eq!(allowed("GPL-3.0-only"), Ok(false));
        assert_eq!(allowed("LGPL-2.1-or-later"), Ok(false));
        assert_eq!(allowed("AGPL-3.0"), Ok(false));
        assert_eq!(allowed("MIT AND GPL-2.0"), Ok(false));
        assert_eq!(allowed("(MIT OR Apache-2.0) AND LGPL-3.0"), Ok(false));
    }
}
