`timescale 1ns / 1ps
module sim_clk_wiz_ps ();

  localparam real ClkInHalfNs = 1.0e3 / 25.6 / 2.0;
  localparam real PsclkPeriodNs = 1.0e3 / 204.8;
  localparam real StepNs = 1.0e3 / (56.0 * 1024.0);
  localparam int TargetSteps = 2800;
  localparam int MaxRate = 18000;
  localparam real RateWindowNs = 100000.0;
  localparam real StepsPerRateUnitPerNs = 204.8e-3 / 262144.0;

  logic clk_in = 1'b0;
  logic reset = 1'b1;
  logic clk;
  logic psclk;
  logic psen;
  logic psincdec;
  logic psdone;
  logic locked;

  clk_wiz clk_wiz (
      .clk_in1(clk_in),
      .clk_out1(clk),
      .clk_out2(psclk),
      .psclk(psclk),
      .psen(psen),
      .psincdec(psincdec),
      .psdone(psdone),
      .reset(reset),
      .locked(locked)
  );

  logic signed [15:0] rate = '0;
  logic rate_toggle = 1'b0;

  sync_ps_nco sync_ps_nco (
      .PSCLK(psclk),
      .RATE(rate),
      .RATE_TOGGLE(rate_toggle),
      .PSDONE(psdone),
      .PSEN(psen),
      .PSINCDEC(psincdec)
  );

  longint n_inc = 0;
  longint n_dec = 0;
  longint n_done = 0;

  always @(posedge psclk) begin
    if (psen & psincdec) n_inc <= n_inc + 1;
    if (psen & ~psincdec) n_dec <= n_dec + 1;
  end

  always @(posedge psdone) n_done <= n_done + 1;

  real  t_psclk_last = 0.0;
  real  raw;
  real  prev_raw;
  real  shift_ns = 0.0;
  real  d;
  logic tracking = 1'b0;
  logic first = 1'b1;

  always @(posedge psclk) t_psclk_last = $realtime;

  always @(posedge clk) begin
    if (tracking) begin
      raw = $realtime - t_psclk_last;
      if (first) begin
        prev_raw = raw;
        first = 1'b0;
      end else begin
        d = raw - prev_raw;
        if (d > PsclkPeriodNs / 2.0) d = d - PsclkPeriodNs;
        if (d < -PsclkPeriodNs / 2.0) d = d + PsclkPeriodNs;
        shift_ns = shift_ns + d;
        prev_raw = raw;
      end
    end
  end

  task set_rate(input logic signed [15:0] r);
    @(posedge clk);
    rate <= r;
    rate_toggle <= ~rate_toggle;
  endtask

  task run_steps(input logic signed [15:0] r, input longint target);
    longint start;
    start = n_inc + n_dec;
    set_rate(r);
    while (n_inc + n_dec - start < target) @(posedge psclk);
    set_rate(16'sd0);
    repeat (256) @(posedge psclk);
  endtask

  task check_shift(input string label);
    real expected;
    expected = (n_inc - n_dec) * StepNs;
    $display("%s: %0d inc, %0d dec, shift %0.4f ns (expected %0.4f ns)", label, n_inc, n_dec, shift_ns, expected);
    if ((shift_ns - expected > 0.01 * StepNs * TargetSteps) || (expected - shift_ns > 0.01 * StepNs * TargetSteps)) begin
      $error("%s:%d: %s: clk_out1 shift: expected is %0.4f ns, but actual is %0.4f ns", `__FILE__, `__LINE__, label, expected, shift_ns);
      $finish();
    end
    if (n_done != n_inc + n_dec) begin
      $error("%s:%d: %s: PSDONE count: expected is %0d, but actual is %0d", `__FILE__, `__LINE__, label, n_inc + n_dec, n_done);
      $finish();
    end
  endtask

  longint steps_start;
  real expected_steps;

  initial begin
    #200;
    reset = 1'b0;
    @(posedge locked);
    repeat (100) @(posedge clk);
    tracking = 1'b1;
    repeat (2000) @(posedge clk);
    check_shift("idle");

    run_steps(MaxRate, TargetSteps);
    check_shift("after advancing");

    run_steps(-MaxRate, TargetSteps);
    check_shift("after retarding");

    set_rate(16'(MaxRate));
    repeat (64) @(posedge psclk);
    steps_start = n_dec;
    #(RateWindowNs);
    expected_steps = MaxRate * StepsPerRateUnitPerNs * RateWindowNs;
    $display("rate %0d: %0d steps in %0.0f ns (expected %0.1f)", MaxRate, n_dec - steps_start, RateWindowNs, expected_steps);
    if (((n_dec - steps_start) < 0.99 * expected_steps) || ((n_dec - steps_start) > 1.01 * expected_steps)) begin
      $error("%s:%d: steps at the maximum rate: expected is %0.1f, but actual is %0d", `__FILE__, `__LINE__, expected_steps, n_dec - steps_start);
      $finish();
    end
    set_rate(16'sd0);
    repeat (256) @(posedge psclk);
    check_shift("after the rate window");

    set_rate(16'(MaxRate));
    repeat (200) @(posedge psclk);
    @(posedge psen);
    #20;
    reset = 1'b1;
    #200;
    reset = 1'b0;
    @(posedge locked);
    repeat (300) @(posedge psclk);
    steps_start = n_inc + n_dec;
    #(RateWindowNs / 10);
    $display("after an MMCM reset mid-step: %0d steps in %0.0f ns", n_inc + n_dec - steps_start, RateWindowNs / 10);
    if ((n_inc + n_dec - steps_start) < 0.9 * expected_steps / 10) begin
      $error("%s:%d: steps after an MMCM reset mid-step: expected is >= %0.1f, but actual is %0d", `__FILE__, `__LINE__, 0.9 * expected_steps / 10,
             n_inc + n_dec - steps_start);
      $finish();
    end
    set_rate(16'sd0);

    $display("OK! sim_clk_wiz_ps");
    $finish();
  end

  always #(ClkInHalfNs) clk_in = ~clk_in;

endmodule
