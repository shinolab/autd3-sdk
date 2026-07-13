use crate::fw;

const FPGA_MAIN_CLK_FREQ: u64 = 20_480_000;
const NUM_BANKS: usize = fw::NUM_BANKS;
const REP_INFINITE: u16 = 0xFFFF;

const MODE_SYNC_IDX: u8 = fw::TRANSITION_MODE_SYNC_IDX;
const MODE_SYS_TIME: u8 = fw::TRANSITION_MODE_SYS_TIME;
const MODE_GPIO: u8 = fw::TRANSITION_MODE_GPIO;
const MODE_EXT: u8 = fw::TRANSITION_MODE_EXT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    WaitStart,
    FiniteLoop,
    InfiniteLoop,
}

pub(crate) struct Swapchain {
    sys_time_ns: u64,
    rep: u16,
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
            state: State::WaitStart,
        }
    }

    pub(crate) fn cur_bank(&self) -> usize {
        self.cur_bank
    }

    pub(crate) fn cur_idx(&self) -> usize {
        self.cur_idx
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn set(
        &mut self,
        sys_time_ns: u64,
        rep: u16,
        freq_div: u16,
        cycle: usize,
        req_bank: usize,
        transition_mode: u8,
        transition_value: u64,
    ) {
        if self.cur_bank == req_bank {
            self.stop = false;
            self.ext_mode = transition_mode == MODE_EXT;
            self.ext_last_lap = self.lap_and_idx(req_bank, sys_time_ns).0;
            self.tic_idx_offset[req_bank] = 0;
            self.state = State::InfiniteLoop;
        } else if rep == REP_INFINITE {
            self.stop = false;
            self.cur_bank = req_bank;
            self.ext_mode = transition_mode == MODE_EXT;
            self.ext_last_lap = self.lap_and_idx(req_bank, sys_time_ns).0;
            self.tic_idx_offset[req_bank] = 0;
            self.state = State::InfiniteLoop;
        } else {
            self.rep = rep;
            self.req_bank = req_bank;
            self.state = State::WaitStart;
        }
        self.sys_time_ns = sys_time_ns;
        self.freq_div[req_bank] = freq_div;
        self.cycle[req_bank] = cycle;
        self.transition_mode = transition_mode;
        self.transition_value = transition_value;
        self.recompute_cur_idx(sys_time_ns);
    }

    pub(crate) fn update(&mut self, gpio_in: [bool; 4], sys_time_ns: u64) {
        let (last_lap, _) = self.lap_and_idx(self.req_bank, self.sys_time_ns);
        let (lap, idx) = self.lap_and_idx(self.req_bank, sys_time_ns);
        match self.state {
            State::WaitStart => {
                let fire = match self.transition_mode {
                    MODE_SYNC_IDX => last_lap < lap,
                    MODE_SYS_TIME => self.transition_value <= sys_time_ns,
                    MODE_GPIO => gpio_in[self.transition_value as usize & 0x3],

                    _ => true,
                };
                if fire {
                    self.stop = false;
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
            State::FiniteLoop => {
                if (self.start_lap[self.cur_bank] + self.rep as usize) + 1 < lap {
                    self.stop = true;
                }
                if (self.start_lap[self.cur_bank] + self.rep as usize) < lap
                    && self.tic_idx_offset[self.cur_bank] <= idx
                {
                    self.stop = true;
                }
            }
            State::InfiniteLoop => {
                if self.ext_mode
                    && self.ext_last_lap < lap
                    && self.ext_last_lap.is_multiple_of(2) != lap.is_multiple_of(2)
                {
                    self.ext_last_lap = lap;
                    self.cur_bank = (self.cur_bank + 1) % NUM_BANKS;
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
        ((u128::from(sys_time_ns) * u128::from(FPGA_MAIN_CLK_FREQ)) / 1_000_000_000) as u64
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

    const MODE_IMMEDIATE: u8 = 0xFF;

    #[test]
    fn cur_idx_stays_within_bank_after_switch_from_large_cycle() {
        let mut sc = Swapchain::new();
        sc.set(0, REP_INFINITE, 1, 65536, 0, MODE_IMMEDIATE, 0);

        let sys_time = 225_000_000;
        sc.update([false; 4], sys_time);
        assert_eq!(0, sc.cur_bank());
        assert!(sc.cur_idx() > 1024, "expected a large stm index on bank 0");

        sc.set(sys_time, REP_INFINITE, 1, 1, 1, MODE_IMMEDIATE, 0);
        assert_eq!(1, sc.cur_bank());
        assert!(
            sc.cur_idx() < sc.cycle[sc.cur_bank()],
            "cur_idx {} must stay within the switched-to bank cycle {}",
            sc.cur_idx(),
            sc.cycle[sc.cur_bank()]
        );
    }
}
