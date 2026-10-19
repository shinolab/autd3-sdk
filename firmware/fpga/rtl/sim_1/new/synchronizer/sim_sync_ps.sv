`timescale 1ns / 1ps
module sim_sync_ps ();

  localparam int NumDevices = 4;
  localparam real SyncPeriodNs = 1.0e6;
  localparam real SyncOffsetNs = 10000.0;
  localparam real SyncHighNs = 100000.0;
  localparam int JitterPs = 5000;
  localparam real GridNs = 25000.0;
  localparam logic [56:0] SYNC_CYCLE_TICKS = 57'(params::SyncCycleTicks);

  localparam real StdBoundNs = 5.0;
  localparam real MeanBoundNs = 3.0;
  localparam int AcquireTimeoutMs = 1500;
  localparam int AcquireStableMs = 20;
  localparam int SettleMs = 300;
  localparam real RateTolerance = 0.15;
  localparam int ManualRate = 5000;
  localparam real StepsPerRateUnitPerMs = 204.8e3 / 262144.0;

  logic PSCLK = 1'b0;
  logic BUS_CLK = 1'b0;
  logic [NumDevices-1:0] bus_en = '0;
  logic bus_we = 1'b0;
  logic [7:0] bus_addr = '0;
  logic [15:0] bus_din = '0;
  logic [15:0] bus_dout[NumDevices];

  logic set = 1'b0;
  logic [63:0] sync_time = '0;
  settings::sync_settings_t SYNC_SETTINGS;
  assign SYNC_SETTINGS.UPDATE = set;
  assign SYNC_SETTINGS.SYNC_TIME = sync_time;

  logic SYNC_IN[NumDevices];
  logic sync_en[NumDevices];
  logic CLK[NumDevices];
  logic [56:0] SYS_TIME[NumDevices];
  logic signed [13:0] SYNC_TIME_DIFF[NumDevices];
  logic [7:0] SYNC_RESYNC_COUNT[NumDevices];
  logic FINE[NumDevices];
  settings::sync_ps_state_t PS_STATE[NumDevices];
  longint PS_STEPS_INC[NumDevices];
  longint PS_STEPS_DEC[NumDevices];

  logic measuring = 1'b0;
  logic watch_fine = 1'b0;
  real st_sum[NumDevices];
  real st_sumsq[NumDevices];
  real st_min[NumDevices];
  real st_max[NumDevices];
  int st_cnt[NumDevices];
  real st_mean[NumDevices];
  real st_std[NumDevices];

  localparam real Ppm[NumDevices] = '{180.0, -100.0, 0.0, 37.0};
  localparam real InitPhase[NumDevices] = '{3.1, 17.7, 29.3, 41.0};

  for (genvar i = 0; i < NumDevices; i++) begin : gen_dev
    sim_helper_sync_ps_device #(
        .PPM(Ppm[i]),
        .INIT_PHASE_NS(InitPhase[i])
    ) dev (
        .SYNC_IN(SYNC_IN[i]),
        .PSCLK(PSCLK),
        .SYNC_SETTINGS(SYNC_SETTINGS),
        .BUS_CLK(BUS_CLK),
        .BUS_EN(bus_en[i]),
        .BUS_WE(bus_we),
        .BUS_ADDR(bus_addr),
        .BUS_DATA_IN(bus_din),
        .BUS_DATA_OUT(bus_dout[i]),
        .CLK(CLK[i]),
        .SYS_TIME(SYS_TIME[i]),
        .SYNC_TIME_DIFF(SYNC_TIME_DIFF[i]),
        .SYNC_RESYNC_COUNT(SYNC_RESYNC_COUNT[i]),
        .FINE(FINE[i]),
        .PS_STATE(PS_STATE[i]),
        .PS_STEPS_INC(PS_STEPS_INC[i]),
        .PS_STEPS_DEC(PS_STEPS_DEC[i])
    );

    initial begin
      integer seed;
      longint k;
      real t;
      seed = 7 + i * 1000;
      k = 0;
      SYNC_IN[i] = 1'b0;
      sync_en[i] = 1'b1;
      forever begin
        t = SyncOffsetNs + k * SyncPeriodNs + $dist_normal(seed, 0, JitterPs) / 1000.0;
        #(t - $realtime) SYNC_IN[i] = sync_en[i];
        #(SyncHighNs) SYNC_IN[i] = 1'b0;
        k++;
      end
    end

    logic [8:0] sys_lo_prev = '0;
    always @(posedge CLK[i]) begin
      real e;
      if (measuring && (SYS_TIME[i][8:0] == 9'd0) && (sys_lo_prev != 9'd0)) begin
        e = $realtime - SyncOffsetNs;
        e = e - GridNs * $floor(e / GridNs + 0.5);
        st_sum[i] += e;
        st_sumsq[i] += e * e;
        st_cnt[i] += 1;
        if (e < st_min[i]) st_min[i] = e;
        if (e > st_max[i]) st_max[i] = e;
      end
      sys_lo_prev <= SYS_TIME[i][8:0];
    end

    always @(negedge SYNC_IN[i]) begin
      if (watch_fine && ((FINE[i] !== 1'b1) || (SYNC_TIME_DIFF[i] != 14'sd0))) begin
        $error(
            "%s:%d: device %0d left the fine loop during the window: expected is FINE=1 and sync_time_diff=0, but actual is FINE=%b sync_time_diff=%0d",
            `__FILE__, `__LINE__, i, FINE[i], SYNC_TIME_DIFF[i]);
        $finish();
      end
    end
  end


  function automatic real abs_real(input real x);
    return (x < 0.0) ? -x : x;
  endfunction

  task stats_reset();
    for (int i = 0; i < NumDevices; i++) begin
      st_sum[i]   = 0.0;
      st_sumsq[i] = 0.0;
      st_cnt[i]   = 0;
      st_min[i]   = 1.0e9;
      st_max[i]   = -1.0e9;
    end
  endtask

  task measure(input string label, input int ms);
    stats_reset();
    measuring = 1'b1;
    #(ms * SyncPeriodNs);
    measuring = 1'b0;
    for (int i = 0; i < NumDevices; i++) begin
      st_mean[i] = st_sum[i] / st_cnt[i];
      st_std[i]  = $sqrt(st_sumsq[i] / st_cnt[i] - st_mean[i] * st_mean[i]);
      $display("%s: device %0d (%0.1f ppm): n=%0d mean=%0.3f ns std=%0.3f ns pk-pk=%0.3f ns", label, i, Ppm[i], st_cnt[i], st_mean[i], st_std[i],
               st_max[i] - st_min[i]);
    end
  endtask

  task bus_write(input logic [NumDevices-1:0] mask, input logic [7:0] addr, input logic [15:0] data);
    @(posedge BUS_CLK);
    bus_en   <= mask;
    bus_we   <= 1'b1;
    bus_addr <= addr;
    bus_din  <= data;
    @(posedge BUS_CLK);
    bus_en <= '0;
    bus_we <= 1'b0;
    repeat (4) @(posedge BUS_CLK);
  endtask

  task bus_read(input int dev, input logic [7:0] addr, output logic [15:0] data);
    @(posedge BUS_CLK);
    bus_en   <= NumDevices'(1) << dev;
    bus_we   <= 1'b0;
    bus_addr <= addr;
    @(posedge BUS_CLK);
    bus_en <= '0;
    @(posedge BUS_CLK);
    data = bus_dout[dev];
  endtask

  task synchronize(input logic [63:0] t);
    @(posedge SYNC_IN[0]);
    #1000;
    sync_time = t;
    set = 1'b1;
    #200;
    set = 1'b0;
    repeat (2) @(negedge SYNC_IN[0]);
  endtask

  task check_sys_time_agree(input string label, input logic [NumDevices-1:0] mask, input int bound);
    longint d;
    @(negedge SYNC_IN[0]);
    #(SyncPeriodNs / 2);
    for (int i = 1; i < NumDevices; i++) begin
      if (mask[i]) begin
        d = longint'(SYS_TIME[i]) - longint'(SYS_TIME[0]);
        if ((d > bound) || (d < -bound)) begin
          $error("%s:%d: %s: device %0d sys_time vs device 0: expected is within +-%0d, but actual is %0d", `__FILE__, `__LINE__, label, i, bound, d);
          $finish();
        end
      end
    end
  endtask

  task wait_all_fine(input string label, input logic [NumDevices-1:0] mask);
    int   stable;
    int   elapsed;
    logic all;
    stable  = 0;
    elapsed = 0;
    while (stable < AcquireStableMs) begin
      @(negedge SYNC_IN[0]);
      elapsed++;
      all = 1'b1;
      for (int i = 0; i < NumDevices; i++) if (mask[i] && (FINE[i] !== 1'b1)) all = 1'b0;
      stable = all ? stable + 1 : 0;
      if (elapsed > AcquireTimeoutMs) begin
        $error("%s:%d: %s: fine lock: expected is within %0d ms, but actual is not locked", `__FILE__, `__LINE__, label, AcquireTimeoutMs);
        $finish();
      end
    end
    $display("%s: fine lock after %0d ms", label, elapsed - AcquireStableMs);
  endtask

  logic [15:0] rd;
  logic [15:0] exit_before[NumDevices];
  longint steps_before;
  longint steps_after;
  real expected_steps;
  real expected_rate;
  longint d_before;
  longint d_after;

  initial begin
    bus_read(0, params::ADDR_SYNC_PS_CTL, rd);
    if (rd != 16'h0001) begin
      $error("%s:%d: SYNC_PS_CTL default: expected is 1, but actual is %0d", `__FILE__, `__LINE__, rd);
      $finish();
    end
    bus_write('1, params::ADDR_SYNC_PS_CTL, 16'h0000);
    repeat (3) @(negedge SYNC_IN[0]);
    synchronize(SYNC_CYCLE_TICKS * 3);
    repeat (10) @(negedge SYNC_IN[0]);
    measure("ps off", 40);

    bus_read(0, params::ADDR_SYNC_PS_KP, rd);
    if (rd != 16'd64) begin
      $error("%s:%d: KP default: expected is 64, but actual is %0d", `__FILE__, `__LINE__, rd);
      $finish();
    end

    bus_write('1, params::ADDR_SYNC_PS_CTL, 16'h0001);
    wait_all_fine("enable", '1);
    repeat (SettleMs) @(negedge SYNC_IN[0]);

    for (int i = 0; i < NumDevices; i++) begin
      bus_read(i, params::ADDR_SYNC_PS_STATE, exit_before[i]);
    end
    watch_fine = 1'b1;
    measure("ps on", 200);
    watch_fine = 1'b0;
    for (int i = 0; i < NumDevices; i++) begin
      if (st_std[i] > StdBoundNs) begin
        $error("%s:%d: device %0d edge std with fine lock: expected is <= %0.1f ns, but actual is %0.3f ns", `__FILE__, `__LINE__, i, StdBoundNs,
               st_std[i]);
        $finish();
      end
      if ((st_mean[i] - st_mean[0] > MeanBoundNs) || (st_mean[0] - st_mean[i] > MeanBoundNs)) begin
        $error("%s:%d: device %0d edge mean vs device 0 with fine lock: expected is within +-%0.1f ns, but actual is %0.3f ns", `__FILE__, `__LINE__,
               i, MeanBoundNs, st_mean[i] - st_mean[0]);
        $finish();
      end
      bus_read(i, params::ADDR_SYNC_PS_STATE, rd);
      if ((rd[0] != 1'b1) || (rd[15:8] != exit_before[i][15:8])) begin
        $error("%s:%d: device %0d SYNC_PS_STATE over the window: expected is fine with %0d exits, but actual is 0x%04x", `__FILE__, `__LINE__, i,
               exit_before[i][15:8], rd);
        $finish();
      end
      bus_read(i, params::ADDR_SYNC_PS_RATE, rd);
      expected_rate = -Ppm[i] * 1.0e-6 * 56.0 * 1024.0e6 * 262144.0 / 204.8e6;
      $display("device %0d rate: %0d (expected %0.0f)", i, $signed(rd), expected_rate);
      if (abs_real($itor($signed(rd)) - expected_rate) > RateTolerance * 7340.0) begin
        $error("%s:%d: device %0d SYNC_PS_RATE: expected is %0.0f, but actual is %0d", `__FILE__, `__LINE__, i, expected_rate, $signed(rd));
        $finish();
      end
    end
    check_sys_time_agree("fine lock", '1, 1);

    @(negedge SYNC_IN[0]);
    sync_en[0] = 1'b0;
    #(5 * SyncPeriodNs);
    sync_en[0] = 1'b1;
    repeat (10) @(negedge SYNC_IN[0]);
    if (SYNC_RESYNC_COUNT[0] != 8'd0) begin
      $error("%s:%d: resync_count after 5 dropped pulses: expected is 0, but actual is %0d", `__FILE__, `__LINE__, SYNC_RESYNC_COUNT[0]);
      $finish();
    end
    wait_all_fine("after 5 dropped pulses", '1);
    check_sys_time_agree("after 5 dropped pulses", '1, 1);

    @(posedge SYNC_IN[1]);
    #(SyncPeriodNs / 2);
    force SYNC_IN[1] = 1'b1;
    #800;
    force SYNC_IN[1] = 1'b0;
    release SYNC_IN[1];
    repeat (3) @(negedge SYNC_IN[0]);
    if (SYNC_RESYNC_COUNT[1] != 8'd2) begin
      $error("%s:%d: resync_count after a spurious pulse: expected is 2, but actual is %0d", `__FILE__, `__LINE__, SYNC_RESYNC_COUNT[1]);
      $finish();
    end
    wait_all_fine("after a spurious pulse", '1);

    synchronize(SYNC_CYCLE_TICKS * 1000);
    wait_all_fine("after Synchronize", '1);
    check_sys_time_agree("after Synchronize", '1, 1);

    @(negedge SYNC_IN[0]);
    #(SyncPeriodNs / 2);
    d_before = longint'(SYS_TIME[2]) - longint'(SYS_TIME[3]);
    bus_write(4'b0100, params::ADDR_SYNC_PS_CTL, 16'h0000);
    repeat (20) @(negedge SYNC_IN[0]);
    #(SyncPeriodNs / 2);
    d_after = longint'(SYS_TIME[2]) - longint'(SYS_TIME[3]);
    if ((d_after - d_before > 2) || (d_before - d_after > 2)) begin
      $error("%s:%d: device 2 sys_time shift on disable: expected is within +-2, but actual is %0d", `__FILE__, `__LINE__, d_after - d_before);
      $finish();
    end
    bus_read(2, params::ADDR_SYNC_PS_RATE, rd);
    if (rd != 16'd0 || FINE[2] !== 1'b0) begin
      $error("%s:%d: device 2 after disable: expected is rate 0 and not fine, but actual is rate %0d fine %b", `__FILE__, `__LINE__, $signed(rd),
             FINE[2]);
      $finish();
    end

    bus_write(4'b1000, params::ADDR_SYNC_PS_MANUAL_RATE, 16'(ManualRate));
    bus_write(4'b1000, params::ADDR_SYNC_PS_CTL, 16'h0003);
    repeat (2) @(negedge SYNC_IN[0]);
    steps_before = PS_STEPS_DEC[3];
    #(10 * SyncPeriodNs);
    steps_after = PS_STEPS_DEC[3];
    expected_steps = ManualRate * StepsPerRateUnitPerMs * 10.0;
    $display("manual: %0d steps in 10 ms (expected %0.1f)", steps_after - steps_before, expected_steps);
    if (abs_real($itor(steps_after - steps_before) - expected_steps) > 0.01 * expected_steps) begin
      $error("%s:%d: manual mode steps in 10 ms: expected is %0.1f, but actual is %0d", `__FILE__, `__LINE__, expected_steps,
             steps_after - steps_before);
      $finish();
    end
    if (FINE[3] !== 1'b0) begin
      $error("%s:%d: device 3 in manual mode: expected is not fine, but actual is fine", `__FILE__, `__LINE__);
      $finish();
    end

    $display("OK! sim_sync_ps");
    $finish();
  end

  always #2.44140625 PSCLK = ~PSCLK;
  always #6.6665 BUS_CLK = ~BUS_CLK;

endmodule
