`timescale 1ns / 1ps
module sim_helper_sync_ps_device #(
    parameter real PPM = 0.0,
    parameter real INIT_PHASE_NS = 0.0
) (
    input wire SYNC_IN,
    input wire PSCLK,
    input settings::sync_settings_t SYNC_SETTINGS,
    input wire BUS_CLK,
    input wire BUS_EN,
    input wire BUS_WE,
    input wire [7:0] BUS_ADDR,
    input wire [15:0] BUS_DATA_IN,
    output var [15:0] BUS_DATA_OUT,
    output var CLK,
    output var [56:0] SYS_TIME,
    output var signed [13:0] SYNC_TIME_DIFF,
    output var [7:0] SYNC_RESYNC_COUNT,
    output var FINE,
    output var signed [15:0] PS_RATE
);

  localparam real ClkPeriodNs = 1.0e9 / 20.48e6;
  localparam real StepNs = 1.0e3 / (56.0 * 1024.0);
  localparam int PsDoneLatency = 12;

  real period_ns;
  real shift_ns = 0.0;
  real next_rise;
  longint n = 0;

  logic psen;
  logic psincdec;
  logic psdone = 1'b0;
  int psdone_cnt = 0;

  logic ps_edge;
  logic signed [13:0] ps_edge_diff;
  logic ps_rate_toggle;
  settings::sync_ps_settings_t PS_SETTINGS;

  sync_ps_regs sync_ps_regs (
      .BUS_CLK(BUS_CLK),
      .CLK(CLK),
      .EN(BUS_EN),
      .WE(BUS_WE),
      .ADDR(BUS_ADDR),
      .DATA_IN(BUS_DATA_IN),
      .DATA_OUT(BUS_DATA_OUT),
      .SETTINGS(PS_SETTINGS)
  );

  initial begin
    period_ns = ClkPeriodNs * (1.0 - PPM * 1.0e-6);
    CLK = 1'b0;
    next_rise = INIT_PHASE_NS;
    forever begin
      #(next_rise - $realtime) CLK = 1'b1;
      #(period_ns / 2.0) CLK = 1'b0;
      n++;
      next_rise = INIT_PHASE_NS + n * period_ns + shift_ns;
    end
  end

  always @(posedge PSCLK) begin
    if (psen) begin
      if (psdone_cnt != 0) begin
        $error("%s:%d: PSEN while a phase shift is in progress: expected is no PSEN, but actual is PSEN", `__FILE__, `__LINE__);
        $finish();
      end
      psdone_cnt <= PsDoneLatency;
      shift_ns   <= psincdec ? shift_ns + StepNs : shift_ns - StepNs;
    end else if (psdone_cnt != 0) begin
      psdone_cnt <= psdone_cnt - 1;
    end
    psdone <= (psdone_cnt == 1);
  end

  synchronizer synchronizer (
      .CLK(CLK),
      .SYNC_SETTINGS(SYNC_SETTINGS),
      .SYNC_IN(SYNC_IN),
      .SYS_TIME(SYS_TIME),
      .SYNC(),
      .SKIP_ONE_ASSERT(),
      .SYNC_TIME_DIFF(SYNC_TIME_DIFF),
      .SYNC_RESYNC_COUNT(SYNC_RESYNC_COUNT),
      .PS_ENABLE(PS_SETTINGS.ENABLE),
      .PS_FINE(FINE),
      .PS_EDGE(ps_edge),
      .PS_EDGE_DIFF(ps_edge_diff)
  );

  sync_ps_ctrl sync_ps_ctrl (
      .CLK(CLK),
      .SETTINGS(PS_SETTINGS),
      .EDGE(ps_edge),
      .FINE(FINE),
      .EDGE_DIFF(ps_edge_diff),
      .RATE(PS_RATE),
      .RATE_TOGGLE(ps_rate_toggle)
  );

  sync_ps_nco sync_ps_nco (
      .PSCLK(PSCLK),
      .RATE(PS_RATE),
      .RATE_TOGGLE(ps_rate_toggle),
      .PSDONE(psdone),
      .PSEN(psen),
      .PSINCDEC(psincdec)
  );

endmodule
