#[derive(Clone)]
pub(crate) struct TocEntry {
    pub(crate) level: u8,
    pub(crate) title: String,
    pub(crate) line: usize,
}

pub(crate) struct TocLevels {
    pub(crate) root: u8,
    pub(crate) sub: Option<u8>,
}

impl TocLevels {
    pub(crate) fn display_level(&self, level: u8) -> Option<u8> {
        if level == self.root {
            Some(1)
        } else if Some(level) == self.sub {
            Some(2)
        } else {
            None
        }
    }
}

fn distinct_levels(toc: &[TocEntry]) -> Vec<u8> {
    let mut levels: Vec<u8> = toc.iter().map(|e| e.level).collect();
    levels.sort_unstable();
    levels.dedup();
    levels
}

pub(crate) fn toc_levels(toc: &[TocEntry]) -> Option<TocLevels> {
    let levels = distinct_levels(toc);
    let &top = levels.first()?;
    let top_unique = toc.iter().filter(|e| e.level == top).count() == 1;
    let root_idx = if top_unique && levels.len() >= 2 {
        1
    } else {
        0
    };
    Some(TocLevels {
        root: levels[root_idx],
        sub: levels.get(root_idx + 1).copied(),
    })
}

pub(crate) fn normalize_toc(mut toc: Vec<TocEntry>) -> Vec<TocEntry> {
    let levels = distinct_levels(&toc);
    let Some(&max_keep) = levels.get(2).or_else(|| levels.last()) else {
        return toc;
    };
    toc.retain(|entry| entry.level <= max_keep);
    toc
}
