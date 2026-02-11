pub type FooterHint = (&'static str, &'static str);

const MOVE: FooterHint = ("Up/Down", "move");
const TOGGLE: FooterHint = ("Space", "toggle");
const ALL: FooterHint = ("a", "all");
const NONE: FooterHint = ("n", "none");
const SEARCH: FooterHint = ("/", "search");
const CANCEL: FooterHint = ("Esc/q", "cancel");
const BACK: FooterHint = ("Backspace/b", "back");

pub fn browse_footer(confirm_label: &'static str) -> Vec<FooterHint> {
    vec![
        MOVE,
        TOGGLE,
        ALL,
        NONE,
        ("Enter", confirm_label),
        SEARCH,
        CANCEL,
    ]
}

pub fn apply_targets_footer() -> Vec<FooterHint> {
    vec![MOVE, ("Enter", "select"), CANCEL]
}

pub fn apply_skills_footer(
    confirm_label: &'static str,
    show_tracking_toggle: bool,
) -> Vec<FooterHint> {
    let mut hints = vec![
        MOVE,
        TOGGLE,
        ALL,
        NONE,
        BACK,
        ("Enter", confirm_label),
        CANCEL,
    ];
    if show_tracking_toggle {
        hints.insert(2, ("g", "git-track"));
    }
    hints
}
