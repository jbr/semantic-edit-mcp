use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Indentation {
    Spaces(u8),
    Tabs,
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
}
