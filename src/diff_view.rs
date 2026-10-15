use similar::{ChangeTag, TextDiff};
#[derive(Clone, Debug)]
pub struct Line {
    pub number: usize,
    pub text: String,
    pub changed: bool,
}
#[derive(Clone, Debug)]
pub struct Row {
    pub source: Option<Line>,
    pub target: Option<Line>,
}
pub fn aligned(source: &str, target: &str) -> Vec<Row> {
    let diff = TextDiff::from_lines(target, source);
    let mut rows = Vec::new();
    let mut removed = Vec::new();
    let mut added = Vec::new();
    let flush = |rows: &mut Vec<Row>, added: &mut Vec<Line>, removed: &mut Vec<Line>| {
        let count = added.len().max(removed.len());
        let mut a = added.drain(..);
        let mut r = removed.drain(..);
        for _ in 0..count {
            rows.push(Row {
                source: a.next(),
                target: r.next(),
            });
        }
    };
    for change in diff.iter_all_changes() {
        let line = change.to_string().trim_end_matches(['\r', '\n']).to_owned();
        match change.tag() {
            ChangeTag::Delete => removed.push(Line {
                number: change.old_index().unwrap() + 1,
                text: line,
                changed: true,
            }),
            ChangeTag::Insert => added.push(Line {
                number: change.new_index().unwrap() + 1,
                text: line,
                changed: true,
            }),
            ChangeTag::Equal => {
                flush(&mut rows, &mut added, &mut removed);
                rows.push(Row {
                    source: Some(Line {
                        number: change.new_index().unwrap() + 1,
                        text: line.clone(),
                        changed: false,
                    }),
                    target: Some(Line {
                        number: change.old_index().unwrap() + 1,
                        text: line,
                        changed: false,
                    }),
                });
            }
        }
    }
    flush(&mut rows, &mut added, &mut removed);
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inserted_lines_keep_following_lines_aligned() {
        let r = aligned("a\ninserted\nb\n", "a\nb\n");
        assert_eq!(r.len(), 3);
        assert!(r[1].target.is_none());
        assert_eq!(r[2].source.as_ref().unwrap().number, 3);
        assert_eq!(r[2].target.as_ref().unwrap().number, 2);
    }
    #[test]
    fn replacements_pair_both_sides() {
        let r = aligned("a\nnew\nz\n", "a\nold\nz\n");
        assert!(r[1].source.as_ref().unwrap().changed);
        assert!(r[1].target.as_ref().unwrap().changed);
        assert!(!r[2].source.as_ref().unwrap().changed);
    }
}
