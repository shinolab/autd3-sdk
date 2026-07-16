`timescale 1ns / 1ps
module sim_swapchain_timer_pending ();

  `include "define.vh"

  localparam int DEPTH = 249;
  localparam int DivLatency = 51;
  localparam int TotalLatency = 1 + 2 * DivLatency + 8 + 1;

  logic CLK;
  logic locked;
  logic [56:0] sys_time;
  sim_helper_clk sim_helper_clk (
      .CLK(CLK),
      .LOCKED(locked),
      .SYS_TIME(sys_time)
  );

  sim_helper_random sim_helper_random ();
  sim_helper_bram #(.DEPTH(DEPTH)) sim_helper_bram ();

  settings::pattern_settings_t pattern_settings;
  logic update_settings;
  logic update_settings_out;
  int out_cnt = 0;

  swapchain_timer swapchain_timer (
      .CLK(CLK),
      .UPDATE_SETTINGS_IN(update_settings),
      .SYS_TIME(sys_time),
      .CYCLE(pattern_settings.CYCLE),
      .FREQ_DIV(pattern_settings.FREQ_DIV),
      .IDX(),
      .UPDATE_SETTINGS_OUT(update_settings_out)
  );

  always @(posedge CLK) begin
    if (update_settings_out) out_cnt <= out_cnt + 1;
  end

  task automatic pulse_update();
    @(posedge CLK);
    update_settings <= 1'b1;
    @(posedge CLK);
    update_settings <= 1'b0;
  endtask

  initial begin
    update_settings = 1'b0;
    pattern_settings.REQ_RD_BANK = 1'b0;
    pattern_settings.CYCLE[0] = 10 - 1;
    pattern_settings.FREQ_DIV[0] = 1;
    pattern_settings.CYCLE[1] = 10 - 1;
    pattern_settings.FREQ_DIV[1] = 1;
    pattern_settings.REP[0] = 16'hFFFF;
    pattern_settings.REP[1] = 16'hFFFF;

    @(posedge locked);

    // a lone update produces exactly one output pulse
    pulse_update();
    repeat (4 * TotalLatency) @(posedge CLK);
    `ASSERT_EQ(1, out_cnt);

    // an update arriving while the previous one is still loading must not be
    // dropped: it is held pending and replayed after the load completes
    pulse_update();
    repeat (20) @(posedge CLK);
    pulse_update();
    repeat (4 * TotalLatency) @(posedge CLK);
    `ASSERT_EQ(3, out_cnt);

    // multiple updates within one load window coalesce into a single replay
    pulse_update();
    repeat (10) @(posedge CLK);
    pulse_update();
    repeat (10) @(posedge CLK);
    pulse_update();
    repeat (6 * TotalLatency) @(posedge CLK);
    `ASSERT_EQ(5, out_cnt);

    $display("OK! sim_swapchain_timer_pending");
    $finish();
  end

endmodule
