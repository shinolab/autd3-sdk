pub struct Component {
    pub name: &'static str,

    pub section: &'static str,

    pub tag_prefix: &'static str,

    pub include_paths: &'static [&'static str],

    pub also_shipped_by: &'static [&'static str],
}

pub const COMPONENTS: &[Component] = &[
    Component {
        name: "software",
        section: "Rust",
        tag_prefix: "v",
        include_paths: &["crates/**", "tools/**", "examples/**", "bindings/ffi/**"],
        also_shipped_by: &[],
    },
    Component {
        name: "python",
        section: "Python",
        tag_prefix: "py-v",
        include_paths: &["bindings/python/**"],
        also_shipped_by: &["v"],
    },
    Component {
        name: "cs",
        section: "C#",
        tag_prefix: "cs-v",
        include_paths: &["bindings/csharp/**"],
        also_shipped_by: &["v"],
    },
    Component {
        name: "simulator",
        section: "Simulator",
        tag_prefix: "simulator-v",
        include_paths: &["simulator/**"],
        also_shipped_by: &["console-v"],
    },
    Component {
        name: "console",
        section: "Console",
        tag_prefix: "console-v",
        include_paths: &["console/**"],
        also_shipped_by: &[],
    },
    Component {
        name: "firmware",
        section: "Firmware",
        tag_prefix: "firmware-v",
        include_paths: &["firmware/**"],
        also_shipped_by: &[],
    },
];

impl Component {
    pub fn tag_pattern(&self) -> String {
        let mut prefixes = vec![self.tag_prefix];
        prefixes.extend_from_slice(self.also_shipped_by);
        format!("^({})[0-9]", prefixes.join("|"))
    }
}

pub fn release_sections(primary: &'static Component) -> Vec<&'static Component> {
    let names: &[&str] = match primary.name {
        "software" => &["software", "python", "cs"],
        "console" => &["console", "simulator"],
        _ => return vec![primary],
    };
    names
        .iter()
        .filter_map(|name| COMPONENTS.iter().find(|c| c.name == *name))
        .collect()
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
