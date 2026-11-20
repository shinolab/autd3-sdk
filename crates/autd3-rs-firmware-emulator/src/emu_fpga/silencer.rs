#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

#[derive(Debug, Clone)]
pub struct SilencerEmulator {
    is_phase: bool,
    current: i32,
    fixed_update_rate_mode: bool,
    update_rate: u16,
    completion_steps: u16,
    current_target: u8,
    diff_mem: u8,
    step_rem_mem: u16,
}

impl SilencerEmulator {
    pub(crate) fn new(
        is_phase: bool,
        initial: u8,
        fixed_update_rate_mode: bool,
        update_rate: u16,
        completion_steps: u16,
    ) -> Self {
        Self {
            is_phase,
            current: i32::from(initial) << 8,
            fixed_update_rate_mode,
            update_rate,
            completion_steps,
            current_target: initial,
            diff_mem: 0,
            step_rem_mem: 0,
        }
    }

    pub(crate) fn is_phase(&self) -> bool {
        self.is_phase
    }

    pub(crate) fn configure(
        &mut self,
        fixed_update_rate_mode: bool,
        update_rate: u16,
        completion_steps: u16,
    ) {
        self.fixed_update_rate_mode = fixed_update_rate_mode;
        self.update_rate = update_rate;
        self.completion_steps = completion_steps;
    }

    fn update_rate(&mut self, input: u8) -> u16 {
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
        let divisor = self.completion_steps.max(1);
        let step_quo = (u16::from(diff) << 8) / divisor;
        let step_rem = (u16::from(diff) << 8) % divisor;
        if rst {
            self.step_rem_mem = step_rem;
        }
        let variable_rate = if self.step_rem_mem == 0 {
            step_quo
        } else {
            self.step_rem_mem -= 1;
            step_quo + 1
        };
        if self.fixed_update_rate_mode {
            self.update_rate
        } else {
            variable_rate
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
        self.current = (self.current + step.clamp(-update_rate, update_rate)) & 0xFFFF;
        (self.current >> 8) as u8
    }
}
