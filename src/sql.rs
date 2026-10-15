//! Conservative SQL tokenization: whitespace/comments may be ignored, literals never are.
use crate::model::{quote, Column};

pub fn tokens(sql: &str, ignore_comments: bool) -> Vec<String> {
    let c: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if c[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if c[i] == '-' && c.get(i + 1) == Some(&'-') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            if !ignore_comments {
                out.push(c[start..i].iter().collect());
            }
            continue;
        }
        if c[i] == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            let mut depth = 1;
            while i < c.len() && depth > 0 {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if !ignore_comments {
                out.push(c[start..i].iter().collect());
            }
            continue;
        }
        if matches!(c[i], '\'' | '"' | '[') {
            let end = if c[i] == '[' { ']' } else { c[i] };
            i += 1;
            while i < c.len() {
                if c[i] == end {
                    i += 1;
                    if c.get(i) == Some(&end) {
                        i += 1;
                    } else {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if c[i].is_alphanumeric() || matches!(c[i], '_' | '@' | '#') {
            i += 1;
            while i < c.len() && (c[i].is_alphanumeric() || matches!(c[i], '_' | '@' | '#' | '$')) {
                i += 1;
            }
        } else {
            i += 1;
        }
        out.push(c[start..i].iter().collect());
    }
    out
}

pub fn normalized(sql: &str, ignore_comments: bool) -> Vec<String> {
    let mut t = tokens(sql, ignore_comments);
    // Only normalize the module header, not identifiers or string literals.
    if t.first().is_some_and(|s| s.eq_ignore_ascii_case("ALTER")) {
        t[0] = "CREATE".into();
    }
    if t.len() > 3
        && t[0].eq_ignore_ascii_case("CREATE")
        && t[1].eq_ignore_ascii_case("OR")
        && t[2].eq_ignore_ascii_case("ALTER")
    {
        t.drain(1..3);
    }
    t
}

pub fn module_ddl(sql: &str, alter: bool) -> anyhow::Result<String> {
    let t = tokens(sql, true);
    let Some(first) = t.first() else {
        anyhow::bail!("Empty SQL module");
    };
    if !first.eq_ignore_ascii_case("CREATE") && !first.eq_ignore_ascii_case("ALTER") {
        anyhow::bail!("Module must start with CREATE or ALTER");
    }
    let mut offset = 1;
    if t.get(1).is_some_and(|s| s.eq_ignore_ascii_case("OR"))
        && t.get(2).is_some_and(|s| s.eq_ignore_ascii_case("ALTER"))
    {
        offset = 3;
    }
    if !t.get(offset).is_some_and(|s| {
        ["VIEW", "PROC", "PROCEDURE", "FUNCTION", "TRIGGER"]
            .iter()
            .any(|x| s.eq_ignore_ascii_case(x))
    }) {
        anyhow::bail!("Unsupported module header");
    }
    // Preserve original body verbatim, including comments and whitespace. Find header
    // outside comments with a scanner, never by searching text inside comments/literals.
    let bytes = sql.as_bytes();
    let mut p = 0;
    loop {
        while bytes.get(p).is_some_and(u8::is_ascii_whitespace) {
            p += 1;
        }
        if bytes.get(p..p + 2) == Some(b"--") {
            while bytes.get(p).is_some_and(|b| *b != b'\n') {
                p += 1;
            }
        } else if bytes.get(p..p + 2) == Some(b"/*") {
            p += 2;
            let mut depth = 1;
            while p < bytes.len() && depth > 0 {
                if bytes.get(p..p + 2) == Some(b"/*") {
                    depth += 1;
                    p += 2;
                } else if bytes.get(p..p + 2) == Some(b"*/") {
                    depth -= 1;
                    p += 2;
                } else {
                    p += 1;
                }
            }
        } else {
            break;
        }
    }
    let start = p;
    // Header tokens can have comments between them; replace only first CREATE/ALTER.
    p += first.len();
    if offset == 3 {
        // CREATE OR ALTER already handles both new and existing objects (SQL 2016 SP1+).
        return Ok(sql.to_owned());
    }
    Ok(format!(
        "{}{}{}",
        &sql[..start],
        if alter { "ALTER" } else { "CREATE" },
        &sql[p..]
    ))
}

pub fn column_sql(c: &Column) -> String {
    if let Some(expr) = &c.computed {
        return format!(
            "{} AS {}{}",
            quote(&c.name),
            expr,
            if c.persisted { " PERSISTED" } else { "" }
        );
    }
    let mut s = format!("{} {}", quote(&c.name), c.data_type);
    if let Some(collation) = &c.collation {
        s.push_str(&format!(" COLLATE {collation}"));
    }
    if let Some(id) = &c.identity {
        s.push_str(&format!(" IDENTITY({id})"));
    }
    s.push_str(if c.nullable { " NULL" } else { " NOT NULL" });
    if let Some(d) = &c.default {
        if let Some(n) = &c.default_name {
            s.push_str(&format!(" CONSTRAINT {}", quote(n)));
        }
        s.push_str(&format!(" DEFAULT {d}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literals_are_not_normalized() {
        assert_ne!(
            normalized("SELECT 'a  b'", true),
            normalized("SELECT 'a b'", true)
        );
        assert_ne!(
            normalized("SELECT 'A'", true),
            normalized("SELECT 'a'", true)
        );
    }
    #[test]
    fn comments_and_escapes() {
        assert_eq!(
            tokens(
                "SELECT /* outer /* inner */ end */ 'it''s--ok', [x]]y] -- hi",
                true
            ),
            vec!["SELECT", "'it''s--ok'", ",", "[x]]y]"]
        );
    }
    #[test]
    fn header_only() {
        assert_eq!(
            module_ddl("-- CREATE\nCREATE PROCEDURE p AS SELECT 'CREATE'", true).unwrap(),
            "-- CREATE\nALTER PROCEDURE p AS SELECT 'CREATE'"
        );
    }
}
