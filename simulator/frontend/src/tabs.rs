#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Slice,
    Camera,
    Field,
    State,
    Settings,
    About,
}

impl Tab {
    pub const ALL: [Tab; 7] = [
        Tab::Home,
        Tab::Slice,
        Tab::Camera,
        Tab::Field,
        Tab::State,
        Tab::Settings,
        Tab::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Slice => "Slice",
            Tab::Field => "Environment",
            Tab::State => "State",
            Tab::Camera => "Camera",
            Tab::Settings => "Settings",
            Tab::About => "About",
        }
    }
}
