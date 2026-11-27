//! The licence check (R6). Every dependency must be usable under a licence
//! that is not GPL, LGPL, or AGPL.

use std::collections::BTreeMap;
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

    if violations.is_empty() {
        if check {
            println!("licence check passed: {} packages", packages.len());
        }
        Ok(())
    } else {
        let list = violations.join("\n    ");
        let message = format!("dependencies that may require (A/L)GPL:\n    {list}");
        if check {
            Err(message)
        } else {
            eprintln!("warning: {message}");
            Ok(())
        }
    }
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
