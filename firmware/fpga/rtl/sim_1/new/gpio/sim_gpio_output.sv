`timescale 1ns / 1ps
module sim_gpio_output ();

  `include "define.vh"

  localparam int DEPTH = 249;

  logic CLK;
  logic locked;
  sim_helper_clk sim_helper_clk (
      .CLK(CLK),
      .LOCKED(locked),
      .SYS_TIME()
  );

  settings::debug_settings_t debug_settings;
  logic [8:0] time_cnt;
  logic [56:0] sys_time;
  logic signed [13:0] sync_time_diff;
  logic pwm_out[DEPTH];
  logic thermo;
  logic force_fan;
  logic sync;
  logic pattern_bank;
  logic mod_bank;
  logic [15:0] pattern_idx;
  logic [15:0] mod_idx;
  logic [15:0] pattern_cycle;
  logic gpio_out[4];

  gpio_output #(
      .DEPTH(DEPTH)
  ) gpio_output (
      .CLK(CLK),
      .DEBUG_SETTINGS(debug_settings),
      .TIME_CNT(time_cnt),
      .SYS_TIME(sys_time),
      .SYNC_TIME_DIFF(sync_time_diff),
      .PWM_OUT(pwm_out),
      .THERMO(thermo),
      .FORCE_FAN(force_fan),
      .SYNC(sync),
      .PATTERN_BANK(pattern_bank),
      .MOD_BANK(mod_bank),
      .PATTERN_IDX(pattern_idx),
      .MOD_IDX(mod_idx),
      .PATTERN_CYCLE(pattern_cycle),
      .GPIO_OUT(gpio_out)
  );

  task automatic check(input logic [7:0] o_type, input logic [55:0] value, input logic expected);
    @(posedge CLK);
    debug_settings.VALUE[0] <= {o_type, value};
    repeat (2) @(posedge CLK);
    @(negedge CLK);
    `ASSERT_EQ(expected, gpio_out[0]);
  endtask

  initial begin
    debug_settings.UPDATE = 1'b0;
    debug_settings.VALUE[0] = {params::GPIO_O_TYPE_NONE, 56'd0};
    debug_settings.VALUE[1] = {params::GPIO_O_TYPE_NONE, 56'd0};
    debug_settings.VALUE[2] = {params::GPIO_O_TYPE_NONE, 56'd0};
    debug_settings.VALUE[3] = {params::GPIO_O_TYPE_NONE, 56'd0};
    time_cnt = 9'h0FF;
    sys_time = {48'h123456789ABC, 9'd5};
    sync_time_diff = 14'sd0;
    for (int i = 0; i < DEPTH; i++) pwm_out[i] = 1'b0;
    thermo = 1'b0;
    force_fan = 1'b0;
    sync = 1'b0;
    pattern_bank = 1'b0;
    mod_bank = 1'b0;
    pattern_idx = 16'd0;
    mod_idx = 16'd0;
    pattern_cycle = 16'd0;

    @(posedge locked);

    check(params::GPIO_O_TYPE_NONE, 56'd0, 1'b0);

    // BASE_SIG = ~TIME_CNT[8]
    time_cnt = 9'h0FF;
    check(params::GPIO_O_TYPE_BASE_SIG, 56'd0, 1'b1);
    time_cnt = 9'h100;
    check(params::GPIO_O_TYPE_BASE_SIG, 56'd0, 1'b0);

    thermo = 1'b1;
    check(params::GPIO_O_TYPE_THERMO, 56'd0, 1'b1);
    thermo = 1'b0;
    check(params::GPIO_O_TYPE_THERMO, 56'd0, 1'b0);

    force_fan = 1'b1;
    check(params::GPIO_O_TYPE_FORCE_FAN, 56'd0, 1'b1);
    force_fan = 1'b0;
    check(params::GPIO_O_TYPE_FORCE_FAN, 56'd0, 1'b0);

    sync = 1'b1;
    check(params::GPIO_O_TYPE_SYNC, 56'd0, 1'b1);
    sync = 1'b0;
    check(params::GPIO_O_TYPE_SYNC, 56'd0, 1'b0);

    mod_bank = 1'b1;
    check(params::GPIO_O_TYPE_MOD_BANK, 56'd0, 1'b1);
    mod_bank = 1'b0;
    check(params::GPIO_O_TYPE_MOD_BANK, 56'd0, 1'b0);

    mod_idx = 16'd42;
    check(params::GPIO_O_TYPE_MOD_IDX, 56'd42, 1'b1);
    check(params::GPIO_O_TYPE_MOD_IDX, 56'd43, 1'b0);

    pattern_bank = 1'b1;
    check(params::GPIO_O_TYPE_PATTERN_BANK, 56'd0, 1'b1);
    pattern_bank = 1'b0;
    check(params::GPIO_O_TYPE_PATTERN_BANK, 56'd0, 1'b0);

    pattern_idx = 16'd7;
    check(params::GPIO_O_TYPE_PATTERN_IDX, 56'd7, 1'b1);
    check(params::GPIO_O_TYPE_PATTERN_IDX, 56'd8, 1'b0);

    pattern_cycle = 16'd2;
    check(params::GPIO_O_TYPE_IS_STM_MODE, 56'd0, 1'b1);
    pattern_cycle = 16'd0;
    check(params::GPIO_O_TYPE_IS_STM_MODE, 56'd0, 1'b0);

    check(params::GPIO_O_TYPE_SYS_TIME_EQ, {8'd0, 48'h123456789ABC}, 1'b1);
    check(params::GPIO_O_TYPE_SYS_TIME_EQ, {8'd0, 48'h123456789ABD}, 1'b0);

    sync_time_diff = 14'sd5;
    check(params::GPIO_O_TYPE_SYNC_DIFF, 56'd0, 1'b1);
    sync_time_diff = -14'sd5;
    check(params::GPIO_O_TYPE_SYNC_DIFF, 56'd0, 1'b1);
    sync_time_diff = 14'sd0;
    check(params::GPIO_O_TYPE_SYNC_DIFF, 56'd0, 1'b0);

    pwm_out[3] = 1'b1;
    check(params::GPIO_O_TYPE_PWM_OUT, 56'd3, 1'b1);
    check(params::GPIO_O_TYPE_PWM_OUT, 56'd4, 1'b0);

    // out-of-range index clamps to DEPTH - 1
    pwm_out[DEPTH-1] = 1'b1;
    check(params::GPIO_O_TYPE_PWM_OUT, 56'd255, 1'b1);
    pwm_out[DEPTH-1] = 1'b0;
    check(params::GPIO_O_TYPE_PWM_OUT, 56'd255, 1'b0);

    check(params::GPIO_O_TYPE_DIRECT, 56'd1, 1'b1);
    check(params::GPIO_O_TYPE_DIRECT, 56'd0, 1'b0);

    // unknown type falls back to 0
    check(8'h7F, 56'd1, 1'b0);

    $display("OK! sim_gpio_output");
    $finish();
  end

endmodule
