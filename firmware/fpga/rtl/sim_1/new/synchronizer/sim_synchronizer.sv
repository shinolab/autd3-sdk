`timescale 1ns / 1ps
module sim_synchronizer ();

  localparam int ECAT_SYNC_BASE = 500000;  // 500 us
  localparam logic [15:0] ECAT_SYNC_CYCLE_TICKS = 4;

  logic CLK_25P6M, CLK_25P6M_p50, CLK_25P6M_m50;

  logic CLK, CLK_p50, CLK_m50;
  logic [56:0] SYS_TIME, SYS_TIME_p50, SYS_TIME_m50;
  logic [56:0] SYS_TIME_WO_SYNC, SYS_TIME_p50_WO_SYNC, SYS_TIME_m50_WO_SYNC;
  logic signed [64:0] diff_p50, diff_m50;

  logic signed [13:0] SYNC_TIME_DIFF, SYNC_TIME_DIFF_p50, SYNC_TIME_DIFF_m50;

  // the -50ppm instance lags, so its corrections are +2 steps flagged by
  // SKIP_ONE_ASSERT; record where in the Sync0 period each one lands.
  logic SKIP_ONE_ASSERT_m50;
  logic [2:0] sync_tri_m50 = '0;
  int period_pos_m50 = 0;
  int corr_total = 0;
  int corr_in_burst_window = 0;
  logic measuring = 1'b0;

  // steady-state bound on |sync_time_diff|. the 14-bit field saturates at 8192;
  // under +-50ppm drift the closed loop keeps the residual to a few ticks, so a
  // margin well below saturation catches both loss-of-lock and future saturation
  // regressions.
  localparam int DiffBound = 64;

  // spread-spectrum regression check: a burst implementation drains the whole
  // diff within ~|diff|+6 clocks of the Sync0 edge, so all corrections would land
  // in this window; with xorshift-randomized intervals (mean 3125 clocks) only a
  // few percent do.
  localparam int BurstWindow = 256;

  logic ECAT_SYNC;

  logic set;
  logic [63:0] ecat_sync_time;  // [ns]
  settings::sync_settings_t SYNC_SETTINGS;
  assign SYNC_SETTINGS.UPDATE = set;
  assign SYNC_SETTINGS.ECAT_SYNC_TIME = ecat_sync_time;
  // ECAT_SYNC_CYCLE_TICKS is the 500us multiple N; the CPU writes N * 10240 ticks.
  assign SYNC_SETTINGS.ECAT_SYNC_CYCLE = 32'd10240 * ECAT_SYNC_CYCLE_TICKS;

  assign diff_p50 = SYS_TIME_p50 - SYS_TIME;
  assign diff_m50 = SYS_TIME_m50 - SYS_TIME;

  clk_wiz clk_wiz (
      .clk_in1(CLK_25P6M),
      .clk_out1(CLK),
      .reset(),
      .locked(lock)
  );

  clk_wiz clk_wiz_p50 (
      .clk_in1(CLK_25P6M_p50),
      .clk_out1(CLK_p50),
      .reset(),
      .locked(lock_p50)
  );

  clk_wiz clk_wiz_m50 (
      .clk_in1(CLK_25P6M_m50),
      .clk_out1(CLK_m50),
      .reset(),
      .locked(lock_m50)
  );

  synchronizer synchronizer (
      .CLK(CLK),
      .SYNC_SETTINGS(SYNC_SETTINGS),
      .ECAT_SYNC(ECAT_SYNC),
      .SYS_TIME(SYS_TIME),
      .SYNC(),
      .SKIP_ONE_ASSERT(),
      .SYNC_TIME_DIFF(SYNC_TIME_DIFF)
  );

  synchronizer synchronizer_p50 (
      .CLK(CLK_p50),
      .SYNC_SETTINGS(SYNC_SETTINGS),
      .ECAT_SYNC(ECAT_SYNC),
      .SYS_TIME(SYS_TIME_p50),
      .SYNC(),
      .SKIP_ONE_ASSERT(),
      .SYNC_TIME_DIFF(SYNC_TIME_DIFF_p50)
  );

  synchronizer synchronizer_m50 (
      .CLK(CLK_m50),
      .SYNC_SETTINGS(SYNC_SETTINGS),
      .ECAT_SYNC(ECAT_SYNC),
      .SYS_TIME(SYS_TIME_m50),
      .SYNC(),
      .SKIP_ONE_ASSERT(SKIP_ONE_ASSERT_m50),
      .SYNC_TIME_DIFF(SYNC_TIME_DIFF_m50)
  );

  always @(posedge CLK_m50) begin
    sync_tri_m50 <= {sync_tri_m50[1:0], ECAT_SYNC};
    if (sync_tri_m50 == 3'b011) period_pos_m50 <= 0;
    else period_pos_m50 <= period_pos_m50 + 1;
    if (measuring & SKIP_ONE_ASSERT_m50) begin
      corr_total <= corr_total + 1;
      if (period_pos_m50 < BurstWindow) corr_in_burst_window <= corr_in_burst_window + 1;
    end
  end

  task sync();
    @(posedge ECAT_SYNC);
    #1000;

    ecat_sync_time = ECAT_SYNC_BASE * 3;
    set = 1;
    @(posedge CLK);
    @(posedge CLK_p50);
    @(posedge CLK_m50);
    set = 0;
    @(negedge ECAT_SYNC);
    SYS_TIME_WO_SYNC <= SYS_TIME;
    SYS_TIME_p50_WO_SYNC <= SYS_TIME_p50;
    SYS_TIME_m50_WO_SYNC <= SYS_TIME_m50;
  endtask

  initial begin
    CLK_25P6M = 1;
    CLK_25P6M_p50 = 1;
    CLK_25P6M_m50 = 1;
    SYS_TIME = 0;
    SYS_TIME_p50 = 0;
    SYS_TIME_m50 = 0;
    SYS_TIME_WO_SYNC = 0;
    SYS_TIME_p50_WO_SYNC = 0;
    SYS_TIME_m50_WO_SYNC = 0;

    set = 0;

    while (~(lock & lock_p50 & lock_m50)) #1000;

    sync();

    // allow the closed loop to settle after the initial coarse sync
    repeat (8) @(negedge ECAT_SYNC);

    measuring = 1;
    for (int i = 0; i < 30; i++) begin
      @(negedge ECAT_SYNC);
      if ((SYNC_TIME_DIFF > DiffBound) || (SYNC_TIME_DIFF < -DiffBound)) begin
        $error("%s:%d: nominal sync_time_diff: expected is within +-%0d, but actual is %0d",
               `__FILE__, `__LINE__, DiffBound, SYNC_TIME_DIFF);
        $finish();
      end
      if ((SYNC_TIME_DIFF_p50 > DiffBound) || (SYNC_TIME_DIFF_p50 < -DiffBound)) begin
        $error("%s:%d: +50ppm sync_time_diff: expected is within +-%0d, but actual is %0d",
               `__FILE__, `__LINE__, DiffBound, SYNC_TIME_DIFF_p50);
        $finish();
      end
      if ((SYNC_TIME_DIFF_m50 > DiffBound) || (SYNC_TIME_DIFF_m50 < -DiffBound)) begin
        $error("%s:%d: -50ppm sync_time_diff: expected is within +-%0d, but actual is %0d",
               `__FILE__, `__LINE__, DiffBound, SYNC_TIME_DIFF_m50);
        $finish();
      end
    end

    if (corr_total == 0) begin
      $error("%s:%d: -50ppm corrections: expected is > 0, but actual is %0d", `__FILE__,
             `__LINE__, corr_total);
      $finish();
    end
    if (2 * corr_in_burst_window >= corr_total) begin
      $error(
          "%s:%d: corrections clustered right after Sync0: expected is < %0d in the first %0d clks, but actual is %0d",
          `__FILE__, `__LINE__, (corr_total + 1) / 2, BurstWindow, corr_in_burst_window);
      $finish();
    end
    $display("corrections: %0d total, %0d within %0d clks of Sync0", corr_total,
             corr_in_burst_window, BurstWindow);

    $display("OK! sim_synchronizer");
    $finish();
  end

  // (1 + 1) / (39.062ns * 1 + 39.063ns * 1) = 25.6MHz
  always begin
    #19.531 CLK_25P6M = ~CLK_25P6M;
    #19.531 CLK_25P6M = ~CLK_25P6M;
    #19.531 CLK_25P6M = ~CLK_25P6M;
    #19.532 CLK_25P6M = ~CLK_25P6M;
  end

  // (10940 + 9061) / (39.061ns * 10940 + 39.060ns * 9061) = 25.6MHz + 50ppm
  always begin
    for (int i = 0; i < 9061; i++) begin
      #19.530 CLK_25P6M_p50 = ~CLK_25P6M_p50;
      #19.530 CLK_25P6M_p50 = ~CLK_25P6M_p50;
      #19.530 CLK_25P6M_p50 = ~CLK_25P6M_p50;
      #19.531 CLK_25P6M_p50 = ~CLK_25P6M_p50;
    end
    for (int i = 0; i < 10940 - 9061; i++) begin
      #19.530 CLK_25P6M_p50 = ~CLK_25P6M_p50;
      #19.531 CLK_25P6M_p50 = ~CLK_25P6M_p50;
    end
  end

  // (9064 + 10935) / (39.065ns * 9064 +  39.064ns * 10935) = 25.6MHz - 50ppm
  always begin
    for (int i = 0; i < 9064; i++) begin
      #19.532 CLK_25P6M_m50 = ~CLK_25P6M_m50;
      #19.533 CLK_25P6M_m50 = ~CLK_25P6M_m50;
      #19.532 CLK_25P6M_m50 = ~CLK_25P6M_m50;
      #19.532 CLK_25P6M_m50 = ~CLK_25P6M_m50;
    end
    for (int i = 0; i < 10935 - 9064; i++) begin
      #19.532 CLK_25P6M_m50 = ~CLK_25P6M_m50;
      #19.533 CLK_25P6M_m50 = ~CLK_25P6M_m50;
    end
  end

  always begin
    #800 ECAT_SYNC = 0;
    #(ECAT_SYNC_BASE * ECAT_SYNC_CYCLE_TICKS - 800) ECAT_SYNC = 1;
  end

  always @(posedge CLK) SYS_TIME_WO_SYNC <= SYS_TIME_WO_SYNC + 1;
  always @(posedge CLK_p50) SYS_TIME_p50_WO_SYNC <= SYS_TIME_p50_WO_SYNC + 1;
  always @(posedge CLK_m50) SYS_TIME_m50_WO_SYNC <= SYS_TIME_m50_WO_SYNC + 1;

endmodule
