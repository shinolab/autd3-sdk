use dioxus::prelude::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const LICENSE: &str = include_str!(concat!(env!("OUT_DIR"), "/license.txt"));
const THIRD_PARTY: &str = include_str!(concat!(env!("OUT_DIR"), "/third-party.md"));

#[component]
pub fn AboutPanel() -> Element {
    rsx! {
        div { class: "px-6 pt-4 flex flex-col gap-4",
            div { class: "card bg-base-100 shadow",
                div { class: "card-body",
                    h2 { class: "card-title", "AUTD3 Simulator" }
                    p { class: "text-sm opacity-70", "version {VERSION}" }
                    a {
                        class: "link link-primary text-sm",
                        href: "https://github.com/shinolab/autd3-sdk",
                        target: "_blank",
                        "github.com/shinolab/autd3-sdk"
                    }
                }
            }
            div { class: "card bg-base-100 shadow",
                div { class: "card-body",
                    h3 { class: "font-semibold", "License (MIT)" }
                    pre {
                        class: "text-xs whitespace-pre-wrap max-h-48 overflow-auto bg-base-200 p-3 rounded",
                        "{LICENSE}"
                    }
                }
            }
            div { class: "card bg-base-100 shadow",
                div { class: "card-body",
                    h3 { class: "font-semibold", "Third-party licenses" }
                    pre {
                        class: "text-xs whitespace-pre-wrap max-h-96 overflow-auto bg-base-200 p-3 rounded",
                        "{THIRD_PARTY}"
                    }
                }
            }
        }
    }
}
