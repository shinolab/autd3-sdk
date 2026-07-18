`timescale 1ns / 1ps
`default_nettype none
module main #(
    parameter int DEPTH = 249
) (
    input wire MRCC_25P6M,
    input wire RESET,
    input wire CAT_SYNC0,
    memory_bus_if.bram_port MEM_BUS,
    input wire THERMO,
    output wire FORCE_FAN,
    output wire PWM_OUT[DEPTH],
    input wire GPIO_IN_HARD[4],
    output wire GPIO_OUT[4]
);

  cnt_bus_if cnt_bus ();
  phase_corr_bus_if phase_corr_bus ();
  output_mask_bus_if output_mask_bus ();
  modulation_bus_if mod_bus ();
  emission_bus_if emission_bus ();
  pwe_table_bus_if pwe_table_bus ();

  settings::mod_settings_t mod_settings;
  settings::pattern_settings_t pattern_settings;
  settings::silencer_settings_t silencer_settings;
  settings::sync_settings_t sync_settings;
  settings::debug_settings_t debug_settings;

  logic clk;
  logic locked;

  logic [56:0] sys_time;
  logic sync;
  logic skip_one_assert;

  logic [8:0] time_cnt;
  logic update;

  logic [7:0] intensity, phase;
  logic dout_valid;

  logic [7:0] intensity_m, phase_m;
  logic dout_valid_m;

  logic [7:0] intensity_s, phase_s;
  logic dout_valid_s;

  logic [8:0] pulse_width_e;
  logic [7:0] phase_e;
  logic dout_valid_e;

  logic [15:0] pattern_idx;
  logic pattern_bank;
  logic [15:0] pattern_cycle;
  logic pattern_stopped;
  logic pattern_transition_pending;
  logic mod_bank;
  logic [15:0] mod_idx;
  logic mod_stopped;
  logic mod_transition_pending;
  logic gpio_in_soft[4];
  logic signed [13:0] sync_time_diff;
  logic [7:0] sync_resync_count;

  (* ASYNC_REG = "true" *) logic gpio_in_hard_meta[4] = '{1'b0, 1'b0, 1'b0, 1'b0};
  (* ASYNC_REG = "true" *) logic gpio_in_hard_sync[4] = '{1'b0, 1'b0, 1'b0, 1'b0};
  logic gpio_in[4];
  for (genvar i = 0; i < 4; i++) begin : gen_gpio_in_cdc
    always_ff @(posedge clk) begin
      gpio_in_hard_meta[i] <= ~GPIO_IN_HARD[i];
      gpio_in_hard_sync[i] <= gpio_in_hard_meta[i];
    end
    assign gpio_in[i] = gpio_in_hard_sync[i] | gpio_in_soft[i];
  end

  (* ASYNC_REG = "true" *)logic thermo_meta = 1'b0;
  (* ASYNC_REG = "true" *)logic thermo_sync = 1'b0;
  always_ff @(posedge clk) begin
    thermo_meta <= THERMO;
    thermo_sync <= thermo_meta;
  end

  clk_wiz clk_wiz (
      .clk_in1(MRCC_25P6M),
      .clk_out1(clk),
      .reset(RESET),
      .locked(locked)
  );

  (* ASYNC_REG = "true" *)logic locked_meta = 1'b0;
  (* ASYNC_REG = "true" *)logic locked_sync = 1'b0;
  always_ff @(posedge clk) begin
    locked_meta <= locked;
    locked_sync <= locked_meta;
  end

  memory memory (
      .CLK(clk),
      .MEM_BUS(MEM_BUS),
      .CNT_BUS(cnt_bus.in_port),
      .PHASE_CORR_BUS(phase_corr_bus.in_port),
      .OUTPUT_MASK_BUS(output_mask_bus.in_port),
      .MOD_BUS(mod_bus.in_port),
      .EMISSION_BUS(emission_bus.in_port),
      .PWE_TABLE_BUS(pwe_table_bus.in_port)
  );

  controller controller (
      .CLK(clk),
      .ENABLE(locked_sync),
      .THERMO(thermo_sync),
      .PATTERN_BANK(pattern_bank),
      .MOD_BANK(mod_bank),
      .PATTERN_CYCLE(pattern_cycle),
      .PATTERN_STOPPED(pattern_stopped),
      .MOD_STOPPED(mod_stopped),
      .TRANSITION_PENDING(pattern_transition_pending | mod_transition_pending),
      .SYNC_RESYNC_COUNT(sync_resync_count),
      .cnt_bus(cnt_bus.out_port),
      .MOD_SETTINGS(mod_settings),
      .PATTERN_SETTINGS(pattern_settings),
      .SILENCER_SETTINGS(silencer_settings),
      .SYNC_SETTINGS(sync_settings),
      .DEBUG_SETTINGS(debug_settings),
      .FORCE_FAN(FORCE_FAN),
      .GPIO_IN(gpio_in_soft)
  );

  synchronizer synchronizer (
      .CLK(clk),
      .SYNC_SETTINGS(sync_settings),
      .ECAT_SYNC(CAT_SYNC0),
      .SYS_TIME(sys_time),
      .SYNC(sync),
      .SKIP_ONE_ASSERT(skip_one_assert),
      .SYNC_TIME_DIFF(sync_time_diff),
      .SYNC_RESYNC_COUNT(sync_resync_count)
  );

  time_cnt_generator time_cnt_generator (
      .CLK(clk),
      .SYS_TIME(sys_time),
      .SKIP_ONE_ASSERT(skip_one_assert),
      .TIME_CNT(time_cnt),
      .UPDATE(update)
  );

  emission #(
      .DEPTH(DEPTH)
  ) emission (
      .CLK(clk),
      .SYS_TIME(sys_time),
      .UPDATE(update),
      .PATTERN_SETTINGS(pattern_settings),
      .EMISSION_BUS(emission_bus.emission_port),
      .EMISSION_BUS_FOCUS(emission_bus.out_focus_port),
      .EMISSION_BUS_RAW(emission_bus.out_raw_port),
      .OUTPUT_MASK_BUS(output_mask_bus.out_port),
      .INTENSITY(intensity),
      .PHASE(phase),
      .GPIO_IN(gpio_in),
      .STOP(pattern_stopped),
      .TRANSITION_PENDING(pattern_transition_pending),
      .DOUT_VALID(dout_valid),
      .DEBUG_IDX(pattern_idx),
      .DEBUG_BANK(pattern_bank),
      .DEBUG_CYCLE(pattern_cycle)
  );

  modulation #(
      .DEPTH(DEPTH)
  ) modulation (
      .CLK(clk),
      .SYS_TIME(sys_time),
      .MOD_SETTINGS(mod_settings),
      .DIN_VALID(dout_valid),
      .INTENSITY_IN(intensity),
      .INTENSITY_OUT(intensity_m),
      .PHASE_IN(phase),
      .PHASE_OUT(phase_m),
      .DOUT_VALID(dout_valid_m),
      .MOD_BUS(mod_bus.out_port),
      .PHASE_CORR_BUS(phase_corr_bus.out_port),
      .GPIO_IN(gpio_in),
      .STOP(mod_stopped),
      .TRANSITION_PENDING(mod_transition_pending),
      .DEBUG_IDX(mod_idx),
      .DEBUG_BANK(mod_bank),
      .DEBUG_STOP()
  );

  silencer #(
      .DEPTH(DEPTH)
  ) silencer (
      .CLK(clk),
      .DIN_VALID(dout_valid_m),
      .SILENCER_SETTINGS(silencer_settings),
      .INTENSITY_IN(intensity_m),
      .PHASE_IN(phase_m),
      .INTENSITY_OUT(intensity_s),
      .PHASE_OUT(phase_s),
      .DOUT_VALID(dout_valid_s)
  );

  pulse_width_encoder #(
      .DEPTH(DEPTH)
  ) pulse_width_encoder (
      .CLK(clk),
      .PWE_TABLE_BUS(pwe_table_bus.out_port),
      .DIN_VALID(dout_valid_s),
      .INTENSITY_IN(intensity_s),
      .PHASE_IN(phase_s),
      .PULSE_WIDTH_OUT(pulse_width_e),
      .PHASE_OUT(phase_e),
      .DOUT_VALID(dout_valid_e)
  );

  pwm #(
      .DEPTH(DEPTH)
  ) pwm (
      .CLK(clk),
      .TIME_CNT(time_cnt),
      .UPDATE(update),
      .DIN_VALID(dout_valid_e),
      .PULSE_WIDTH(pulse_width_e),
      .PHASE(phase_e),
      .PWM_OUT(PWM_OUT),
      .DOUT_VALID()
  );

  gpio_output #(
      .DEPTH(DEPTH)
  ) gpio_output (
      .CLK(clk),
      .DEBUG_SETTINGS(debug_settings),
      .TIME_CNT(time_cnt),
      .SYS_TIME(sys_time),
      .SYNC_TIME_DIFF(sync_time_diff),
      .PWM_OUT(PWM_OUT),
      .THERMO(thermo_sync),
      .FORCE_FAN(FORCE_FAN),
      .SYNC(sync),
      .PATTERN_BANK(pattern_bank),
      .MOD_BANK(mod_bank),
      .PATTERN_IDX(pattern_idx),
      .MOD_IDX(mod_idx),
      .PATTERN_CYCLE(pattern_cycle),
      .GPIO_OUT(GPIO_OUT)
  );

endmodule
`default_nettype wire
