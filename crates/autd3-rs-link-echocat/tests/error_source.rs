use autd3_rs_core::error::LinkError;
use autd3_rs_core::geometry::{Autd3, Geometry};
use autd3_rs_core::link::IntoLink;
use autd3_rs_link_echocat::{EchocatError, EchocatLinkOption};

fn echocat_cause(e: &LinkError) -> &EchocatError {
    let source = core::error::Error::source(e).expect("into_link must keep the cause");
    source
        .downcast_ref::<EchocatError>()
        .unwrap_or_else(|| panic!("the cause must stay an EchocatError, got {source}"))
}

#[test]
fn opening_an_interface_that_cannot_be_used_keeps_the_echocat_error() {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = EchocatLinkOption {
        iface: "autd3-no-such-interface".into(),
        ..EchocatLinkOption::default()
    };

    let e = option
        .into_link(&geometry)
        .err()
        .expect("no such interface exists");

    assert!(
        matches!(echocat_cause(&e), EchocatError::Io(_)),
        "a refused or missing interface must arrive as EchocatError::Io, got {e}"
    );
}

#[test]
fn an_invalid_option_keeps_the_echocat_error() {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = EchocatLinkOption {
        sync0_period: core::time::Duration::ZERO,
        ..EchocatLinkOption::default()
    };

    let e = option
        .into_link(&geometry)
        .err()
        .expect("a zero sync0 period is rejected");

    assert!(
        matches!(echocat_cause(&e), EchocatError::InvalidOption { .. }),
        "option validation must arrive as EchocatError::InvalidOption, got {e}"
    );
}
