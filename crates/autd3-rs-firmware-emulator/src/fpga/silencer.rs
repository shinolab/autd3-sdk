#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

#[derive(Debug, Clone, Copy)]
pub struct SilencerEmulator {
    is_phase: bool,
    current: i32,
    fixed_update_rate_mode: bool,
    value: u16,
    current_target: u8,
    diff_mem: u8,
    step_rem_mem: u16,
}

impl SilencerEmulator {
    pub(crate) fn new(
        is_phase: bool,
        initial: u8,
        fixed_update_rate_mode: bool,
        value: u16,
    ) -> Self {
        Self {
            is_phase,
            current: i32::from(initial) << 8,
            fixed_update_rate_mode,
            value,
            current_target: initial,
            diff_mem: 0,
            step_rem_mem: 0,
        }
    }

    fn update_rate(&mut self, input: u8) -> u16 {
        if self.fixed_update_rate_mode {
            return self.value;
        }
        let mut diff = self.current_target.abs_diff(input);
        self.current_target = input;
        if self.is_phase && diff >= 128 {
            diff = (256 - u16::from(diff)) as u8;
        }
        let (diff, rst) = if diff == 0 {
            (self.diff_mem, false)
        } else {
            self.diff_mem = diff;
            (diff, true)
        };
        let step_quo = (u16::from(diff) << 8) / self.value;
        let step_rem = (u16::from(diff) << 8) % self.value;
        if rst {
            self.step_rem_mem = step_rem;
            step_quo
        } else if self.step_rem_mem == 0 {
            step_quo
        } else {
            self.step_rem_mem -= 1;
            step_quo + 1
        }
    }

    pub fn apply(&mut self, input: u8) -> u8 {
        let update_rate = i32::from(self.update_rate(input));
        let mut step = (i32::from(input) << 8) - self.current;
        if self.is_phase {
            step = if step < 0 {
                if step >= -32768 { step } else { step + 65536 }
            } else if step <= 32768 {
                step
            } else {
                step - 65536
            };
        }
        if step < 0 {
            if -update_rate <= step {
                self.current += step;
            } else {
                self.current -= update_rate;
            }
        } else if step <= update_rate {
            self.current += step;
        } else {
            self.current += update_rate;
        }
        (self.current >> 8) as u8
    }
}
