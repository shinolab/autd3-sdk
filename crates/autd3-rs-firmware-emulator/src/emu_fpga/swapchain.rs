use crate::fw;
use crate::fw::{FPGA_CLK_FREQ_HZ, NUM_BANKS, REP_INFINITE};

const MODE_SYNC_IDX: u8 = fw::TransitionModeKind::SyncIdx.as_u8();
const MODE_SYS_TIME: u8 = fw::TransitionModeKind::SysTime.as_u8();
const MODE_GPIO: u8 = fw::TransitionModeKind::Gpio.as_u8();
const MODE_EXT: u8 = fw::TransitionModeKind::Ext.as_u8();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    WaitStart,
    FiniteLoop,
    InfiniteLoop,
}

pub(crate) struct Swapchain {
    sys_time_ns: u64,
    rep: u16,
    req_rep: u16,
    playing_finite: bool,
    start_lap: [usize; NUM_BANKS],
    freq_div: [u16; NUM_BANKS],
    cycle: [usize; NUM_BANKS],
    tic_idx_offset: [usize; NUM_BANKS],
    cur_bank: usize,
    req_bank: usize,
    cur_idx: usize,
    transition_mode: u8,
    transition_value: u64,
    stop: bool,
    ext_mode: bool,
    ext_last_lap: usize,
    state: State,
}

impl Swapchain {
    pub(crate) fn new() -> Self {
        Self {
            sys_time_ns: 0,
            rep: 0,
            req_rep: 0,
            playing_finite: false,
            start_lap: [0; NUM_BANKS],
            freq_div: [10; NUM_BANKS],
            cycle: [1; NUM_BANKS],
            tic_idx_offset: [0; NUM_BANKS],
            cur_bank: 0,
            req_bank: 0,
            cur_idx: 0,
            transition_mode: MODE_EXT,
            transition_value: 0,
            stop: false,
            ext_mode: false,
            ext_last_lap: 0,
            state: State::InfiniteLoop,
        }
    }

    pub(crate) fn cur_bank(&self) -> usize {
        self.cur_bank
    }

    #[cfg(test)]
    pub(crate) fn force_cur_bank(&mut self, bank: usize) {
        self.cur_bank = bank;
    }

    pub(crate) fn cur_idx(&self) -> usize {
        self.cur_idx
    }

    pub(crate) fn stopped(&self) -> bool {
        self.stop
    }

    pub(crate) fn ext_active(&self) -> bool {
        self.ext_mode && self.state == State::InfiniteLoop
    }

    pub(crate) fn transition_pending(&self) -> bool {
        self.state == State::WaitStart
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn set(
        &mut self,
        sys_time_ns: u64,
        rep: u16,
        freq_div: [u16; NUM_BANKS],
        cycle: [usize; NUM_BANKS],
        req_bank: usize,
        transition_mode: u8,
        transition_value: u64,
    ) {
        let target_bank = if self.state == State::WaitStart {
            self.req_bank
        } else {
            self.cur_bank
        };
        self.freq_div = freq_div;
        self.cycle = cycle;
        if rep == REP_INFINITE {
            self.stop = false;
            self.playing_finite = false;
            self.cur_bank = req_bank;
            self.ext_mode = transition_mode == MODE_EXT;
            self.ext_last_lap = self.lap_and_idx(req_bank, sys_time_ns).0;
            self.tic_idx_offset = [0; NUM_BANKS];
            self.state = State::InfiniteLoop;
        } else if req_bank != target_bank {
            self.req_rep = rep;
            self.req_bank = req_bank;
            self.sys_time_ns = sys_time_ns;
            self.transition_mode = transition_mode;
            self.transition_value = transition_value;
            self.state = State::WaitStart;
        }
        self.recompute_cur_idx(sys_time_ns);
    }

    pub(crate) fn update(&mut self, gpio_in: [bool; 4], sys_time_ns: u64) {
        if self.playing_finite && self.cycle[self.cur_bank] > 1 {
            let (lap, idx) = self.lap_and_idx(self.cur_bank, sys_time_ns);
            if (self.start_lap[self.cur_bank] + self.rep as usize) + 1 < lap {
                self.stop = true;
            }
            if (self.start_lap[self.cur_bank] + self.rep as usize) < lap
                && self.tic_idx_offset[self.cur_bank] <= idx
            {
                self.stop = true;
            }
        }
        let (last_lap, last_idx) = self.lap_and_idx(self.req_bank, self.sys_time_ns);
        let (lap, idx) = self.lap_and_idx(self.req_bank, sys_time_ns);
        self.sys_time_ns = sys_time_ns;
        match self.state {
            State::WaitStart => {
                let idx_changed =
                    self.cycle[self.req_bank] > 1 && (last_lap, last_idx) != (lap, idx);
                let fire = match self.transition_mode {
                    MODE_SYNC_IDX => idx_changed && last_lap < lap,
                    MODE_SYS_TIME => self.transition_value <= Self::fpga_sys_time(sys_time_ns),
                    MODE_GPIO => idx_changed && gpio_in[self.transition_value as usize & 0x3],
                    _ => false,
                };
                if fire {
                    self.stop = false;
                    self.rep = self.req_rep;
                    self.playing_finite = true;
                    self.start_lap[self.req_bank] = lap;
                    self.tic_idx_offset[self.req_bank] = if self.transition_mode == MODE_SYNC_IDX {
                        0
                    } else {
                        idx
                    };
                    self.cur_bank = self.req_bank;
                    self.state = State::FiniteLoop;
                }
            }
            State::FiniteLoop => {}
            State::InfiniteLoop => {
                let (lap, _) = self.lap_and_idx(self.cur_bank, sys_time_ns);
                if self.ext_mode
                    && self.cycle[self.cur_bank] > 1
                    && self.ext_last_lap < lap
                    && self.ext_last_lap.is_multiple_of(2) != lap.is_multiple_of(2)
                {
                    self.cur_bank = (self.cur_bank + 1) % NUM_BANKS;
                    self.ext_last_lap = self.lap_and_idx(self.cur_bank, sys_time_ns).0;
                }
            }
        }
        self.recompute_cur_idx(sys_time_ns);
    }

    fn recompute_cur_idx(&mut self, sys_time_ns: u64) {
        let (_, idx) = self.lap_and_idx(self.cur_bank, sys_time_ns);
        let cycle = self.cycle[self.cur_bank].max(1);
        self.cur_idx = if self.stop {
            cycle - 1
        } else {
            (idx + cycle - self.tic_idx_offset[self.cur_bank]) % cycle
        };
    }

    fn fpga_sys_time(sys_time_ns: u64) -> u64 {
        ((u128::from(sys_time_ns) * u128::from(FPGA_CLK_FREQ_HZ)) / 1_000_000_000) as u64
    }

    fn lap_and_idx(&self, bank: usize, sys_time_ns: u64) -> (usize, usize) {
        let freq_div = u64::from(self.freq_div[bank]).max(1);
        let a = ((Self::fpga_sys_time(sys_time_ns) >> 9) / freq_div) as usize;
        let cycle = self.cycle[bank].max(1);
        (a / cycle, a % cycle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODE_IMMEDIATE: u8 = fw::TransitionModeKind::Immediate.as_u8();

    #[test]
    fn cur_idx_stays_within_bank_after_switch_from_large_cycle() {
        let mut sc = Swapchain::new();
        sc.set(
            0,
            REP_INFINITE,
            [1; NUM_BANKS],
            [65536, 1],
            0,
            MODE_IMMEDIATE,
            0,
        );

        let sys_time = 225_000_000;
        sc.update([false; 4], sys_time);
        assert_eq!(0, sc.cur_bank());
        assert!(sc.cur_idx() > 1024, "expected a large stm index on bank 0");

        sc.set(
            sys_time,
            REP_INFINITE,
            [1; NUM_BANKS],
            [65536, 1],
            1,
            MODE_IMMEDIATE,
            0,
        );
        assert_eq!(1, sc.cur_bank());
        assert!(
            sc.cur_idx() < sc.cycle[sc.cur_bank()],
            "cur_idx {} must stay within the switched-to bank cycle {}",
            sc.cur_idx(),
            sc.cycle[sc.cur_bank()]
        );
    }

    const CYCLE: usize = 4;
    const LAP_NS: u64 = 100_000;

    fn run(sc: &mut Swapchain, from_ns: u64, to_ns: u64) {
        let mut t = from_ns;
        while t <= to_ns {
            sc.update([false; 4], t);
            t += LAP_NS / 8;
        }
    }

    fn start_finite_on_bank0(sc: &mut Swapchain, rep: u16) -> u64 {
        sc.set(
            0,
            REP_INFINITE,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            1,
            MODE_IMMEDIATE,
            0,
        );
        sc.set(
            LAP_NS / 2,
            rep,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_SYNC_IDX,
            0,
        );
        assert!(sc.transition_pending());
        run(sc, LAP_NS / 2, LAP_NS + LAP_NS / 8);
        assert_eq!(0, sc.cur_bank());
        assert!(!sc.transition_pending());
        assert!(!sc.stopped());
        LAP_NS + LAP_NS / 8
    }

    #[test]
    fn stopped_finite_bank_is_not_replayed_by_other_bank_config() {
        let mut sc = Swapchain::new();
        let t = start_finite_on_bank0(&mut sc, 0);
        run(&mut sc, t, 3 * LAP_NS);
        assert!(sc.stopped());
        assert_eq!(CYCLE - 1, sc.cur_idx());

        sc.set(
            3 * LAP_NS,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_SYNC_IDX,
            0,
        );
        let mut t = 3 * LAP_NS;
        while t <= 10 * LAP_NS {
            sc.update([false; 4], t);
            assert_eq!(0, sc.cur_bank());
            assert!(sc.stopped());
            assert!(!sc.transition_pending());
            assert_eq!(CYCLE - 1, sc.cur_idx());
            t += LAP_NS / 8;
        }
    }

    #[test]
    fn running_finite_loop_is_not_restarted_by_same_bank_request() {
        let mut sc = Swapchain::new();
        let t = start_finite_on_bank0(&mut sc, 1);
        run(&mut sc, t, 2 * LAP_NS + LAP_NS / 2);
        assert!(!sc.stopped());

        sc.set(
            2 * LAP_NS + LAP_NS / 2,
            1,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_SYNC_IDX,
            0,
        );
        assert!(!sc.transition_pending());
        run(
            &mut sc,
            2 * LAP_NS + LAP_NS / 2,
            2 * LAP_NS + 7 * LAP_NS / 8,
        );
        assert!(!sc.stopped());
        run(&mut sc, 3 * LAP_NS, 3 * LAP_NS + LAP_NS / 8);
        assert!(sc.stopped());
        assert_eq!(0, sc.cur_bank());
    }

    #[test]
    fn pending_transition_is_replaced_by_request_to_previous_bank() {
        let mut sc = Swapchain::new();
        sc.set(
            0,
            REP_INFINITE,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_IMMEDIATE,
            0,
        );
        sc.set(
            LAP_NS / 2,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            1,
            MODE_SYS_TIME,
            Swapchain::fpga_sys_time(1_000 * LAP_NS),
        );
        assert!(sc.transition_pending());

        sc.set(
            LAP_NS / 2 + 1,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            1,
            MODE_SYNC_IDX,
            0,
        );
        run(&mut sc, LAP_NS / 2 + 1, 2 * LAP_NS + LAP_NS / 2);
        assert_eq!(0, sc.cur_bank());
        assert!(sc.transition_pending());

        sc.set(
            2 * LAP_NS + LAP_NS / 2,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_SYNC_IDX,
            0,
        );
        assert!(sc.transition_pending());
        run(&mut sc, 2 * LAP_NS + LAP_NS / 2, 3 * LAP_NS + LAP_NS / 8);
        assert_eq!(0, sc.cur_bank());
        assert!(!sc.transition_pending());
        assert!(!sc.stopped());
    }

    #[test]
    fn ext_mode_ignores_tic_offset_left_by_previous_finite_loop() {
        let mut sc = Swapchain::new();
        sc.set(
            0,
            REP_INFINITE,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_IMMEDIATE,
            0,
        );
        sc.set(
            LAP_NS / 2,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            1,
            MODE_SYS_TIME,
            0,
        );
        sc.update([false; 4], LAP_NS / 2);
        assert_eq!(1, sc.cur_bank());
        assert_ne!(0, sc.tic_idx_offset[1]);

        sc.set(
            LAP_NS,
            REP_INFINITE,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_EXT,
            0,
        );
        run(&mut sc, LAP_NS, 2 * LAP_NS + LAP_NS / 2);
        assert_eq!(1, sc.cur_bank());
        assert_eq!(2, sc.cur_idx());
    }

    #[test]
    fn sync_idx_and_gpio_never_fire_into_single_entry_bank() {
        for mode in [MODE_SYNC_IDX, MODE_GPIO] {
            let mut sc = Swapchain::new();
            sc.set(
                0,
                REP_INFINITE,
                [1; NUM_BANKS],
                [CYCLE, 1],
                0,
                MODE_IMMEDIATE,
                0,
            );
            sc.set(LAP_NS / 2, 0, [1; NUM_BANKS], [CYCLE, 1], 1, mode, 0);
            let mut t = LAP_NS / 2;
            while t <= 10 * LAP_NS {
                sc.update([true; 4], t);
                t += LAP_NS / 8;
            }
            assert_eq!(0, sc.cur_bank());
            assert!(sc.transition_pending());
        }
    }

    #[test]
    fn finite_loop_on_single_entry_bank_never_stops() {
        let mut sc = Swapchain::new();
        sc.set(
            0,
            REP_INFINITE,
            [1; NUM_BANKS],
            [CYCLE, 1],
            0,
            MODE_IMMEDIATE,
            0,
        );
        sc.set(
            LAP_NS / 2,
            0,
            [1; NUM_BANKS],
            [CYCLE, 1],
            1,
            MODE_SYS_TIME,
            0,
        );
        run(&mut sc, LAP_NS / 2, 10 * LAP_NS);
        assert_eq!(1, sc.cur_bank());
        assert!(!sc.transition_pending());
        assert!(!sc.stopped());
    }

    #[test]
    fn ext_mode_never_leaves_single_entry_bank() {
        let mut sc = Swapchain::new();
        sc.set(0, REP_INFINITE, [1; NUM_BANKS], [1, CYCLE], 0, MODE_EXT, 0);
        run(&mut sc, 0, 10 * LAP_NS);
        assert_eq!(0, sc.cur_bank());

        let mut sc = Swapchain::new();
        sc.set(0, REP_INFINITE, [1; NUM_BANKS], [CYCLE, 1], 0, MODE_EXT, 0);
        run(&mut sc, 0, LAP_NS + LAP_NS / 8);
        assert_eq!(1, sc.cur_bank());
        run(&mut sc, LAP_NS + LAP_NS / 8, 10 * LAP_NS);
        assert_eq!(1, sc.cur_bank());
    }

    #[test]
    fn finite_loop_with_immediate_or_ext_stays_pending_until_infinite_request() {
        for mode in [MODE_IMMEDIATE, MODE_EXT] {
            let mut sc = Swapchain::new();
            sc.set(
                0,
                REP_INFINITE,
                [1; NUM_BANKS],
                [CYCLE; NUM_BANKS],
                0,
                MODE_IMMEDIATE,
                0,
            );
            sc.set(
                LAP_NS / 2,
                0,
                [1; NUM_BANKS],
                [CYCLE; NUM_BANKS],
                1,
                mode,
                0,
            );
            assert!(sc.transition_pending());
            run(&mut sc, LAP_NS / 2, 10 * LAP_NS);
            assert_eq!(0, sc.cur_bank());
            assert!(sc.transition_pending());

            sc.set(
                10 * LAP_NS,
                REP_INFINITE,
                [1; NUM_BANKS],
                [CYCLE; NUM_BANKS],
                1,
                MODE_IMMEDIATE,
                0,
            );
            assert_eq!(1, sc.cur_bank());
            assert!(!sc.transition_pending());
        }
    }

    #[test]
    fn tic_finite_loop_stops_at_its_rep_while_other_bank_pending() {
        let mut sc = Swapchain::new();
        sc.set(
            0,
            REP_INFINITE,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            1,
            MODE_IMMEDIATE,
            0,
        );
        sc.set(
            LAP_NS / 2,
            1,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_SYS_TIME,
            0,
        );
        sc.update([false; 4], LAP_NS / 2);
        assert_eq!(0, sc.cur_bank());
        assert!(!sc.transition_pending());

        sc.set(
            LAP_NS / 2,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            1,
            MODE_SYS_TIME,
            Swapchain::fpga_sys_time(1_000 * LAP_NS),
        );
        assert!(sc.transition_pending());
        run(&mut sc, LAP_NS / 2, 2 * LAP_NS + LAP_NS / 4);
        assert!(!sc.stopped());
        run(&mut sc, 2 * LAP_NS + LAP_NS / 2, 3 * LAP_NS);
        assert_eq!(0, sc.cur_bank());
        assert!(sc.transition_pending());
        assert!(sc.stopped());
    }

    #[test]
    fn gpio_transition_fires_on_idx_change_not_on_level() {
        let mut sc = Swapchain::new();
        sc.set(
            0,
            REP_INFINITE,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_IMMEDIATE,
            0,
        );
        sc.set(
            LAP_NS / 2 + 1_000,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            1,
            MODE_GPIO,
            0,
        );

        sc.update([true, false, false, false], LAP_NS / 2 + 2_000);
        assert_eq!(0, sc.cur_bank());
        assert!(sc.transition_pending());

        sc.update([false; 4], 3 * LAP_NS / 4 + 1_000);
        assert!(sc.transition_pending());

        sc.update([true, false, false, false], 3 * LAP_NS / 4 + 2_000);
        assert!(sc.transition_pending());

        sc.update([true, false, false, false], LAP_NS + 1_000);
        assert_eq!(1, sc.cur_bank());
        assert!(!sc.transition_pending());
        assert_eq!(0, sc.cur_idx());
    }

    #[test]
    fn finite_loop_with_immediate_pending_is_replaced_by_firing_mode_to_other_bank() {
        let mut sc = Swapchain::new();
        sc.set(
            0,
            REP_INFINITE,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_IMMEDIATE,
            0,
        );
        sc.set(
            LAP_NS / 2,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            1,
            MODE_IMMEDIATE,
            0,
        );
        run(&mut sc, LAP_NS / 2, 2 * LAP_NS);
        assert!(sc.transition_pending());

        sc.set(
            2 * LAP_NS,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            1,
            MODE_SYS_TIME,
            0,
        );
        sc.update([false; 4], 2 * LAP_NS);
        assert_eq!(0, sc.cur_bank());
        assert!(sc.transition_pending());

        sc.set(
            2 * LAP_NS,
            0,
            [1; NUM_BANKS],
            [CYCLE; NUM_BANKS],
            0,
            MODE_SYS_TIME,
            0,
        );
        sc.update([false; 4], 2 * LAP_NS);
        assert_eq!(0, sc.cur_bank());
        assert!(!sc.transition_pending());
        assert!(!sc.stopped());
    }
}
