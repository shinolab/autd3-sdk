`timescale 1ns / 1ps
module sim_synchronizer ();

  localparam int ECAT_SYNC_BASE = 500000;  // 500 us
  localparam logic [15:0] ECAT_SYNC_CYCLE_TICKS = 4;

  logic CLK, CLK_p50, CLK_m50;
  logic lock, lock_p50, lock_m50;
  logic [56:0] SYS_TIME, SYS_TIME_p50, SYS_TIME_m50;
  logic [56:0] SYS_TIME_WO_SYNC, SYS_TIME_p50_WO_SYNC, SYS_TIME_m50_WO_SYNC;
  logic signed [64:0] diff_p50, diff_m50;

  logic signed [13:0] SYNC_TIME_DIFF, SYNC_TIME_DIFF_p50, SYNC_TIME_DIFF_m50;

  logic [7:0] SYNC_RESYNC_COUNT;

  logic SKIP_ONE_ASSERT_m50;
  logic [2:0] sync_tri_m50 = '0;
  int period_pos_m50 = 0;
  int corr_total = 0;
  int corr_in_burst_window = 0;
  logic measuring = 1'b0;

  localparam int DiffBound = 64;

  localparam int BurstWindow = 256;

  logic ECAT_SYNC;
  logic ecat_sync_en = 1'b1;

  logic set;
  logic [63:0] ecat_sync_time;  // [ns]
  settings::sync_settings_t SYNC_SETTINGS;
  assign SYNC_SETTINGS.UPDATE = set;
  assign SYNC_SETTINGS.ECAT_SYNC_TIME = ecat_sync_time;
  assign SYNC_SETTINGS.ECAT_SYNC_CYCLE = 32'd10240 * ECAT_SYNC_CYCLE_TICKS;

  assign diff_p50 = SYS_TIME_p50 - SYS_TIME;
  assign diff_m50 = SYS_TIME_m50 - SYS_TIME;

  synchronizer synchronizer (
      .CLK(CLK),
      .SYNC_SETTINGS(SYNC_SETTINGS),
      .ECAT_SYNC(ECAT_SYNC),
      .SYS_TIME(SYS_TIME),
      .SYNC(),
      .SKIP_ONE_ASSERT(),
      .SYNC_TIME_DIFF(SYNC_TIME_DIFF),
      .SYNC_RESYNC_COUNT(SYNC_RESYNC_COUNT)
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

  task check_diff_converged(input string label);
    for (int i = 0; i < 4; i++) begin
      @(negedge ECAT_SYNC);
      if ((SYNC_TIME_DIFF > DiffBound) || (SYNC_TIME_DIFF < -DiffBound)) begin
        $error("%s:%d: nominal sync_time_diff %s: expected is within +-%0d, but actual is %0d", `__FILE__, `__LINE__, label, DiffBound,
               SYNC_TIME_DIFF);
        $finish();
      end
      if ((SYNC_TIME_DIFF_p50 > DiffBound) || (SYNC_TIME_DIFF_p50 < -DiffBound)) begin
        $error("%s:%d: +50ppm sync_time_diff %s: expected is within +-%0d, but actual is %0d", `__FILE__, `__LINE__, label, DiffBound,
               SYNC_TIME_DIFF_p50);
        $finish();
      end
      if ((SYNC_TIME_DIFF_m50 > DiffBound) || (SYNC_TIME_DIFF_m50 < -DiffBound)) begin
        $error("%s:%d: -50ppm sync_time_diff %s: expected is within +-%0d, but actual is %0d", `__FILE__, `__LINE__, label, DiffBound,
               SYNC_TIME_DIFF_m50);
        $finish();
      end
    end
  endtask

  initial begin
    CLK = 1;
    CLK_p50 = 1;
    CLK_m50 = 1;
    lock = 0;
    lock_p50 = 0;
    lock_m50 = 0;
    #500;
    lock = 1;
    lock_p50 = 1;
    lock_m50 = 1;
  end

  initial begin
    SYS_TIME = 0;
    SYS_TIME_p50 = 0;
    SYS_TIME_m50 = 0;
    SYS_TIME_WO_SYNC = 0;
    SYS_TIME_p50_WO_SYNC = 0;
    SYS_TIME_m50_WO_SYNC = 0;

    set = 0;

    while (~(lock & lock_p50 & lock_m50)) #1000;

    sync();

    repeat (8) @(negedge ECAT_SYNC);

    measuring = 1;
    for (int i = 0; i < 30; i++) begin
      @(negedge ECAT_SYNC);
      if ((SYNC_TIME_DIFF > DiffBound) || (SYNC_TIME_DIFF < -DiffBound)) begin
        $error("%s:%d: nominal sync_time_diff: expected is within +-%0d, but actual is %0d", `__FILE__, `__LINE__, DiffBound, SYNC_TIME_DIFF);
        $finish();
      end
      if ((SYNC_TIME_DIFF_p50 > DiffBound) || (SYNC_TIME_DIFF_p50 < -DiffBound)) begin
        $error("%s:%d: +50ppm sync_time_diff: expected is within +-%0d, but actual is %0d", `__FILE__, `__LINE__, DiffBound, SYNC_TIME_DIFF_p50);
        $finish();
      end
      if ((SYNC_TIME_DIFF_m50 > DiffBound) || (SYNC_TIME_DIFF_m50 < -DiffBound)) begin
        $error("%s:%d: -50ppm sync_time_diff: expected is within +-%0d, but actual is %0d", `__FILE__, `__LINE__, DiffBound, SYNC_TIME_DIFF_m50);
        $finish();
      end
    end

    if (corr_total == 0) begin
      $error("%s:%d: -50ppm corrections: expected is > 0, but actual is %0d", `__FILE__, `__LINE__, corr_total);
      $finish();
    end
    if (2 * corr_in_burst_window >= corr_total) begin
      $error("%s:%d: corrections clustered right after Sync0: expected is < %0d in the first %0d clks, but actual is %0d", `__FILE__, `__LINE__,
             (corr_total + 1) / 2, BurstWindow, corr_in_burst_window);
      $finish();
    end
    $display("corrections: %0d total, %0d within %0d clks of Sync0", corr_total, corr_in_burst_window, BurstWindow);

    measuring = 0;

    // Nominal operation must never trip the hard-resync path.
    if (SYNC_RESYNC_COUNT != 8'd0) begin
      $error("%s:%d: resync_count before anomalies: expected is 0, but actual is %0d", `__FILE__, `__LINE__, SYNC_RESYNC_COUNT);
      $finish();
    end

    // A single dropped Sync0 pulse must not leave sync_time_diff saturated forever:
    // sys_time free-runs, so the missing edge is a period slip and the synchronizer
    // hard-rebuilds next_sync_time on the first edge back, converging to ~0.
    @(negedge ECAT_SYNC);
    ecat_sync_en = 0;
    #(ECAT_SYNC_BASE * ECAT_SYNC_CYCLE_TICKS);
    ecat_sync_en = 1;
    repeat (2) @(negedge ECAT_SYNC);
    check_diff_converged("after dropped Sync0");
    // Exactly one slip -> exactly one hard-resync counted.
    if (SYNC_RESYNC_COUNT != 8'd1) begin
      $error("%s:%d: resync_count after dropped Sync0: expected is 1, but actual is %0d", `__FILE__, `__LINE__, SYNC_RESYNC_COUNT);
      $finish();
    end

    // A single spurious Sync0 edge is an over-count; it must also resync to ~0.
    @(negedge ECAT_SYNC);
    #(ECAT_SYNC_BASE * ECAT_SYNC_CYCLE_TICKS / 2);
    force ECAT_SYNC = 1;
    #800;
    force ECAT_SYNC = 0;
    release ECAT_SYNC;
    repeat (3) @(negedge ECAT_SYNC);
    check_diff_converged("after spurious Sync0");
    // The spurious edge (and the now-early real edge) trip further resyncs.
    if (SYNC_RESYNC_COUNT <= 8'd1) begin
      $error("%s:%d: resync_count after spurious Sync0: expected is > 1, but actual is %0d", `__FILE__, `__LINE__, SYNC_RESYNC_COUNT);
      $finish();
    end

    @(posedge ECAT_SYNC);
    #(ECAT_SYNC_BASE * ECAT_SYNC_CYCLE_TICKS - 2000);
    ecat_sync_time = ECAT_SYNC_BASE * 7;
    set = 1;
    @(posedge CLK);
    @(posedge CLK_p50);
    @(posedge CLK_m50);
    set = 0;

    repeat (2) @(negedge ECAT_SYNC);
    check_diff_converged("after racing update");
    // Synchronize re-establishes the time base, so the slip count starts a fresh epoch.
    if (SYNC_RESYNC_COUNT != 8'd0) begin
      $error("%s:%d: resync_count after racing update: expected is 0, but actual is %0d", `__FILE__, `__LINE__, SYNC_RESYNC_COUNT);
      $finish();
    end

    $display("OK! sim_synchronizer");
    $finish();
  end

  always #24.414 CLK = ~CLK;

  always #24.413 CLK_p50 = ~CLK_p50;

  always #24.415 CLK_m50 = ~CLK_m50;

  always begin
    #800 ECAT_SYNC = 0;
    #(ECAT_SYNC_BASE * ECAT_SYNC_CYCLE_TICKS - 800) ECAT_SYNC = ecat_sync_en;
  end

  always @(posedge CLK) SYS_TIME_WO_SYNC <= SYS_TIME_WO_SYNC + 1;
  always @(posedge CLK_p50) SYS_TIME_p50_WO_SYNC <= SYS_TIME_p50_WO_SYNC + 1;
  always @(posedge CLK_m50) SYS_TIME_m50_WO_SYNC <= SYS_TIME_m50_WO_SYNC + 1;

endmodule
