#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Wide,
    Medium,
    Narrow,
}

pub fn resolve_width(raw: Option<usize>) -> usize {
    if let Some(width) = raw {
        if width > 0 {
            return width;
        }
    }
    if let Some((terminal_size::Width(width), _)) = terminal_size::terminal_size() {
        if width > 0 {
            return width as usize;
        }
    }
    120
}

pub fn resolve_density(width: usize) -> Density {
    if width >= 120 {
        Density::Wide
    } else if width >= 90 {
        Density::Medium
    } else {
        Density::Narrow
    }
}
