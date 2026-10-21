#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Home,
    Geometry,
    Slice,
    Camera,
    Field,
    State,
    Settings,
    About,
}

impl Tab {
    pub(crate) const ALL: [Tab; 8] = [
        Tab::Home,
        Tab::Geometry,
        Tab::Slice,
        Tab::Camera,
        Tab::Field,
        Tab::State,
        Tab::Settings,
        Tab::About,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Geometry => "Geometry",
            Tab::Slice => "Slice",
            Tab::Field => "Environment",
            Tab::State => "State",
            Tab::Camera => "Camera",
            Tab::Settings => "Settings",
            Tab::About => "About",
        }
    }
}
