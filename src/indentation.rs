use std::{
    borrow::Cow,
    collections::BTreeMap,
    fmt::{self, Display, Formatter, Write},
};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Indentation {
    Spaces(u8),
    Tabs,
}

impl Display for Indentation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Indentation::Spaces(spaces) => {
                for _ in 0..*spaces {
                    f.write_char(' ')?;
                }
                Ok(())
            }
            Indentation::Tabs => f.write_char('\t'),
        }
    }
}

struct LineIndent {
    indentation: Indentation,
    count: usize,
}

impl Display for LineIndent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for _ in 0..self.count {
            Display::fmt(&self.indentation, f)?;
        }

        Ok(())
    }
}

impl Indentation {
    fn counts(source: &str) -> BTreeMap<Self, usize> {
        let mut counts = BTreeMap::new();
        let mut last_indentation = 0;
        for line in source.lines().take(100) {
            if line.starts_with('\t') {
                *counts.entry(Self::Tabs).or_default() += 1;
            } else {
                let current_indentation = line.chars().take_while(|c| c == &' ').count();
                if line.len() != current_indentation {
                    // ignore indentation-only lines
                    let diff = current_indentation.abs_diff(last_indentation);
                    last_indentation = current_indentation;
                    if let Ok(diff) = u8::try_from(diff) {
                        if diff > 0 {
                            *counts.entry(Self::Spaces(diff)).or_default() += 1;
                        }
                    }
                }
            }
        }
        counts
    }

    pub fn determine(source: &str) -> Option<Self> {
        Self::counts(source)
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(spaces, _)| spaces)
    }

    pub fn minimum(&self, source: &str) -> usize {
        source
            .lines()
            .filter(|s| !s.trim().is_empty())
            .map(|line| self.unit_count(line))
            .min()
            .unwrap_or(0)
    }

    /// Reindent and normalize content to a specific base level and indentation style while
    /// preserving relative indentation
    pub fn reindent<'a>(
        &self,
        target_indentation_count: usize,
        content: &mut Cow<'a, str>,
        indent_first_line: bool,
    ) {
        if content.is_empty() {
            return;
        }

        let (first_line, content_to_consider) = if indent_first_line {
            ("", &**content)
        } else if let Some(first_line_end) = content.find('\n') {
            content.split_at(first_line_end)
        } else {
            //just one line and we don't want to reindent it
            return;
        };

        let content_counts = Self::counts(content_to_consider);

        let content_style = content_counts
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(spaces, _)| *spaces)
            .unwrap_or(Self::Spaces(4));

        let content_indentation = content_to_consider
            .lines()
            .map(|line| (content_style.unit_count(line), line))
            .collect::<Vec<_>>();

        let min_units = content_indentation
            .iter()
            .filter(|(_, s)| !s.trim().is_empty())
            .map(|(x, _)| *x)
            .min()
            .unwrap_or(0);

        let consistent_style = content_counts.len() == 1;

        if self == &content_style && min_units == target_indentation_count && consistent_style {
            return;
        }

        let mut string = String::from(first_line);
        for (current_units, line) in content_indentation {
            let relative_units = current_units.saturating_sub(min_units);
            let new_units = target_indentation_count + relative_units;
            let new_indentation = LineIndent {
                indentation: *self,
                count: new_units,
            };
            let line = line.trim_start();
            writeln!(&mut string, "{new_indentation}{line}").unwrap();
        }

        if !content.ends_with('\n') {
            string.pop();
        }

        // log::trace!(
        //     "reindented from {min_units} {content_style:?} to {target_indentation_count} {self:?}"
        // );

        *content = Cow::Owned(string);
    }

    // fn convert_line_indentation(&self, line: &str, from_style: &Indentation) -> String {
    //     if line.trim().is_empty() {
    //         return line.to_string();
    //     }

    //     let units = from_style.line_indentation(line);
    //     let new_indentation = self.create_indentation(units);
    //     format!("{}{}", new_indentation, line.trim_start())
    // }

    pub fn unit_count(&self, line: &str) -> usize {
        match self {
            Indentation::Spaces(n) => {
                if *n == 0 {
                    0
                } else {
                    let spaces = line.chars().take_while(|c| *c == ' ').count();
                    spaces.div_ceil(*n as usize)
                }
            }
            Indentation::Tabs => line.chars().take_while(|c| *c == '\t').count(),
        }
    }

    // fn create_indentation(&self, units: usize) -> String {
    //     match self {
    //         Indentation::Spaces(n) => " ".repeat(*n as usize * units),
    //         Indentation::Tabs => "\t".repeat(units),
    //     }
    // }
}
