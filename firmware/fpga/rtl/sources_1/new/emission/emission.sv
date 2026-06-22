`timescale 1ns / 1ps
module emission #(
    parameter int DEPTH = 249,
    parameter string MODE = "NearestEven"
) (
    input wire CLK,
    input wire [56:0] SYS_TIME,
    input wire UPDATE,
    input settings::pattern_settings_t PATTERN_SETTINGS,
    emission_bus_if.emission_port EMISSION_BUS,
    emission_bus_if.out_focus_port EMISSION_BUS_FOCUS,
    emission_bus_if.out_raw_port EMISSION_BUS_RAW,
    output_mask_bus_if.out_port OUTPUT_MASK_BUS,
    output wire [7:0] INTENSITY,
    output wire [7:0] PHASE,
    output wire DOUT_VALID,
    input wire GPIO_IN[4],
    output wire [15:0] DEBUG_IDX,
    output wire DEBUG_BANK,
    output wire [15:0] DEBUG_CYCLE
);

  logic mode = params::EMISSION_TYPE_RAW;
  logic start = 1'b0;
  logic bank = '0;
  logic [15:0] idx = '0;
  logic [15:0] cycle = '0;
  logic [15:0] sound_speed = '0;
  logic [7:0] num_foci = 8'd1;

  assign EMISSION_BUS.MODE = mode;
  assign EMISSION_BUS.BANK = bank;
  assign OUTPUT_MASK_BUS.BANK = bank;

  logic update_settings;
  logic [7:0] intensity_raw;
  logic [7:0] phase_raw;
  logic [7:0] intensity_focus;
  logic [7:0] phase_focus;
  logic dout_valid_raw, dout_valid_focus;
  logic [7:0] intensity;
  logic [7:0] phase;
  logic dout_valid;

  assign intensity = mode == params::EMISSION_TYPE_RAW ? intensity_raw : intensity_focus;
  assign phase = mode == params::EMISSION_TYPE_RAW ? phase_raw : phase_focus;
  assign dout_valid = mode == params::EMISSION_TYPE_RAW ? dout_valid_raw : dout_valid_focus;

  assign DEBUG_IDX = idx;
  assign DEBUG_BANK = bank;
  assign DEBUG_CYCLE = cycle;

  logic [15:0] timer_idx[params::NumBanks];
  emission_timer emission_timer (
      .CLK(CLK),
      .UPDATE_SETTINGS_IN(PATTERN_SETTINGS.UPDATE),
      .SYS_TIME(SYS_TIME),
      .CYCLE(PATTERN_SETTINGS.CYCLE),
      .FREQ_DIV(PATTERN_SETTINGS.FREQ_DIV),
      .IDX(timer_idx),
      .UPDATE_SETTINGS_OUT(update_settings)
  );

  logic [15:0] swapchain_idx[params::NumBanks];
  logic swapchain_bank;
  logic swapchain_stop;
  emission_swapchain emission_swapchain (
      .CLK(CLK),
      .SYS_TIME(SYS_TIME),
      .UPDATE_SETTINGS(update_settings),
      .REQ_RD_BANK(PATTERN_SETTINGS.REQ_RD_BANK),
      .TRANSITION_MODE(PATTERN_SETTINGS.TRANSITION_MODE),
      .TRANSITION_VALUE(PATTERN_SETTINGS.TRANSITION_VALUE),
      .CYCLE(PATTERN_SETTINGS.CYCLE),
      .REP(PATTERN_SETTINGS.REP),
      .SYNC_IDX(timer_idx),
      .GPIO_IN(GPIO_IN),
      .STOP(swapchain_stop),
      .BANK(swapchain_bank),
      .IDX(swapchain_idx)
  );

  emission_raw #(
      .DEPTH(DEPTH)
  ) emission_raw (
      .CLK(CLK),
      .START(start),
      .IDX(idx),
      .EMISSION_BUS(EMISSION_BUS_RAW),
      .INTENSITY(intensity_raw),
      .PHASE(phase_raw),
      .DOUT_VALID(dout_valid_raw)
  );

  emission_focus #(
      .DEPTH(DEPTH),
      .MODE (MODE)
  ) emission_focus (
      .CLK(CLK),
      .START(start),
      .IDX(idx),
      .EMISSION_BUS(EMISSION_BUS_FOCUS),
      .SOUND_SPEED(sound_speed),
      .NUM_FOCI(num_foci),
      .INTENSITY(intensity_focus),
      .PHASE(phase_focus),
      .DOUT_VALID(dout_valid_focus)
  );

  always_ff @(posedge CLK) begin
    if (UPDATE) begin
      if (swapchain_stop == 1'b0) begin
        bank <= swapchain_bank;
        idx <= swapchain_idx[swapchain_bank];
        mode <= PATTERN_SETTINGS.MODE[swapchain_bank];
        sound_speed <= PATTERN_SETTINGS.SOUND_SPEED[swapchain_bank];
        cycle <= PATTERN_SETTINGS.CYCLE[swapchain_bank];
        num_foci <= PATTERN_SETTINGS.NUM_FOCI[swapchain_bank];
      end
      start <= 1'b1;
    end else begin
      start <= 1'b0;
    end
  end

  output_mask #(
      .DEPTH(DEPTH)
  ) output_mask (
      .CLK(CLK),
      .MASK_VALUE(OUTPUT_MASK_BUS.VALUE),
      .DIN_VALID(dout_valid),
      .INTENSITY_IN(intensity),
      .INTENSITY_OUT(INTENSITY),
      .PHASE_IN(phase),
      .PHASE_OUT(PHASE),
      .DOUT_VALID(DOUT_VALID)
  );

endmodule
