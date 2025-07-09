use std::collections::BTreeMap;

pub(super) enum Indentation {
    Spaces(u8),
    Tabs,
}

impl Indentation {
    pub fn determine(source: &str) -> Option<Self> {
        let mut tab_count = 0;
        let mut space_counts = BTreeMap::<u8, usize>::new();
        let mut last_indentation = 0;
        let mut last_change = 0;
        for line in source.lines().take(100) {
            if line.starts_with('\t') {
                tab_count += 1;
            } else {
                let count = line.chars().take_while(|c| c == &' ').count();
                let diff = count.abs_diff(last_indentation);
                last_indentation = count;
                if diff > 0 {
                    last_change = diff;
                }
                if let Ok(last_change) = u8::try_from(last_change) {
                    let entry = space_counts.entry(last_change).or_default();
                    *entry += 1;
                }
            }
        }

        space_counts
            .into_iter()
            .map(|(k, v)| (Some(k), v))
            .chain(std::iter::once((None, tab_count)))
            .max_by_key(|(_, count)| *count)
            .map(|(spaces, _)| spaces.map(Self::Spaces).unwrap_or(Self::Tabs))
    }
}
