use crate::params::{
    TRANSITION_MODE_EXT, TRANSITION_MODE_GPIO, TRANSITION_MODE_IMMEDIATE, TRANSITION_MODE_SYNC_IDX,
    TRANSITION_MODE_SYS_TIME,
};

crate::wire_enum! {
    pub enum TransitionMode {
        SyncIdx = TRANSITION_MODE_SYNC_IDX,
        SysTime = TRANSITION_MODE_SYS_TIME,
        Gpio = TRANSITION_MODE_GPIO,
        Ext = TRANSITION_MODE_EXT,
        Immediate = TRANSITION_MODE_IMMEDIATE,
    }
}
