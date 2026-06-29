pub struct Component {
    pub name: &'static str,

    pub section: &'static str,

    pub tag_prefix: &'static str,

    pub include_paths: &'static [&'static str],
}

pub const COMPONENTS: &[Component] = &[
    Component {
        name: "software",
        section: "Software",
        tag_prefix: "v",
        include_paths: &["crates/**", "tools/**", "examples/**", "bindings/**"],
    },
    Component {
        name: "python",
        section: "Python",
        tag_prefix: "py-v",
        include_paths: &["bindings/python/**"],
    },
    Component {
        name: "cs",
        section: "C#",
        tag_prefix: "cs-v",
        include_paths: &["bindings/csharp/**"],
    },
    Component {
        name: "simulator",
        section: "Simulator",
        tag_prefix: "simulator-v",
        include_paths: &["simulator/**"],
    },
    Component {
        name: "console",
        section: "Console",
        tag_prefix: "console-v",
        include_paths: &["console/**"],
    },
    Component {
        name: "firmware",
        section: "Firmware",
        tag_prefix: "firmware-v",
        include_paths: &["firmware/**"],
    },
];

impl Component {
    pub fn tag_pattern(&self) -> String {
        format!("^{}[0-9]", self.tag_prefix)
    }
}

pub fn detect<'a>(versioned: &'a str) -> Option<(&'static Component, &'a str)> {
    let mut best: Option<(&'static Component, &'a str)> = None;
    for c in COMPONENTS {
        if let Some(rest) = versioned.strip_prefix(c.tag_prefix)
            && best.is_none_or(|(b, _)| c.tag_prefix.len() > b.tag_prefix.len())
        {
            best = Some((c, rest));
        }
    }
    best
}
