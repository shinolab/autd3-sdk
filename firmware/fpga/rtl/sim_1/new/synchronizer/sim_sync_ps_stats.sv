`timescale 1ns / 1ps
module sim_sync_ps_stats ();

  `include "define.vh"

  import params::*;

  logic CLK;
  logic locked;

  sim_helper_bram sim_helper_bram ();

  sim_helper_clk sim_helper_clk (
      .CLK(CLK),
      .LOCKED(locked),
      .SYS_TIME()
  );

  cnt_bus_if cnt_bus ();
  phase_corr_bus_if phase_corr_bus ();
  modulation_bus_if mod_bus ();
  emission_bus_if emission_bus ();
  pwe_table_bus_if pwe_table_bus ();
  output_mask_bus_if output_mask_bus ();
  flash_bus_if flash_bus ();

  settings::sync_ps_status_t status;

  memory memory (
      .CLK(CLK),
      .MEM_BUS(sim_helper_bram.memory_bus.bram_port),
      .CNT_BUS(cnt_bus.in_port),
      .PHASE_CORR_BUS(phase_corr_bus.in_port),
      .OUTPUT_MASK_BUS(output_mask_bus.in_port),
      .MOD_BUS(mod_bus.in_port),
      .EMISSION_BUS(emission_bus.in_port),
      .PWE_TABLE_BUS(pwe_table_bus.in_port),
      .FLASH_BUS(flash_bus.host_port),
      .CPU_WATCHDOG_FIRED(),
      .SYNC_PS_SETTINGS(),
      .SYNC_PS_STATUS(status)
  );

  assign cnt_bus.WE   = 1'b0;
  assign cnt_bus.ADDR = 8'd0;
  assign cnt_bus.DIN  = 16'd0;

  localparam logic [15:0] CmdSnapshot = 16'h0001;
  localparam logic [15:0] CmdClear = 16'h0002;

  typedef struct {
    logic signed [15:0] diff_min;
    logic signed [15:0] diff_max;
    logic [15:0] diff_outliers;
    logic signed [15:0] rate_min;
    logic signed [15:0] rate_max;
    logic [15:0] fine_exits;
  } stats_t;

  initial begin
    status.EDGE = 1'b0;
    status.FINE = 1'b0;
    status.EDGE_DIFF = '0;
    status.RATE = '0;
  end

  task automatic pulse_edge(input int diff);
    @(negedge CLK);
    status.EDGE = 1'b1;
    status.EDGE_DIFF = 14'(diff);
    @(negedge CLK);
    status.EDGE = 1'b0;
  endtask

  task automatic set_rate(input int rate);
    @(negedge CLK);
    status.RATE = 16'(rate);
  endtask

  task automatic set_fine(input logic fine);
    @(negedge CLK);
    status.FINE = fine;
  endtask

  task automatic command(input logic [15:0] cmd);
    logic [15:0] busy;
    sim_helper_bram.bram_write(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_STATS_CTL, cmd);
    do begin
      @(posedge CLK);
      sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_STATS_CTL, busy);
    end while (busy[0]);
  endtask

  task automatic read_stats(output stats_t s, input int max_gap);
    logic [15:0] v;
    repeat ($urandom_range(max_gap)) @(posedge CLK);
    sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_DIFF_MIN, v);
    s.diff_min = v;
    repeat ($urandom_range(max_gap)) @(posedge CLK);
    sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_DIFF_MAX, v);
    s.diff_max = v;
    repeat ($urandom_range(max_gap)) @(posedge CLK);
    sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_DIFF_OUTLIERS, v);
    s.diff_outliers = v;
    repeat ($urandom_range(max_gap)) @(posedge CLK);
    sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_RATE_MIN, v);
    s.rate_min = v;
    repeat ($urandom_range(max_gap)) @(posedge CLK);
    sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_RATE_MAX, v);
    s.rate_max = v;
    repeat ($urandom_range(max_gap)) @(posedge CLK);
    sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_FINE_EXITS, v);
    s.fine_exits = v;
  endtask

  task automatic check_stats(input int diff_min, input int diff_max, input int diff_outliers, input int rate_min, input int rate_max,
                             input int fine_exits);
    stats_t s;
    read_stats(s, 0);
    `ASSERT_EQ(diff_min, int'(s.diff_min));
    `ASSERT_EQ(diff_max, int'(s.diff_max));
    `ASSERT_EQ(diff_outliers, int'(s.diff_outliers));
    `ASSERT_EQ(rate_min, int'(s.rate_min));
    `ASSERT_EQ(rate_max, int'(s.rate_max));
    `ASSERT_EQ(fine_exits, int'(s.fine_exits));
  endtask

  stats_t expected;
  int snapshots = 0;

  always @(posedge CLK) begin
    if (memory.sync_ps_regs.stats_req_pulse && memory.sync_ps_regs.stats_cmd[0]) begin
      expected.diff_min = memory.sync_ps_regs.sync_ps_stats.diff_min;
      expected.diff_max = memory.sync_ps_regs.sync_ps_stats.diff_max;
      expected.diff_outliers = memory.sync_ps_regs.sync_ps_stats.diff_outliers;
      expected.rate_min = memory.sync_ps_regs.sync_ps_stats.rate_min;
      expected.rate_max = memory.sync_ps_regs.sync_ps_stats.rate_max;
      expected.fine_exits = memory.sync_ps_regs.sync_ps_stats.fine_exits;
      snapshots++;
    end
  end

  logic random_drive = 1'b0;

  always @(negedge CLK) begin
    if (random_drive) begin
      status.EDGE = $urandom_range(3) == 0;
      status.EDGE_DIFF = 14'($signed($urandom_range(40)) - 20);
      status.RATE = 16'($signed($urandom_range(36000)) - 18000);
      if ($urandom_range(15) == 0) status.FINE = ~status.FINE;
    end
  end

  logic [15:0] value;
  stats_t got;
  int live_moved;

  initial begin
    @(posedge locked);
    repeat (16) @(posedge CLK);

    sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_STATS_CTL, value);
    `ASSERT_EQ(16'h0000, value);
    check_stats(32767, -32768, 0, 0, 0, 0);

    set_fine(1'b1);
    set_rate(100);
    command(CmdClear);
    `ASSERT_EQ(0, snapshots);
    check_stats(32767, -32768, 0, 0, 0, 0);
    for (int i = 0; i < 20; i++) begin
      set_rate(100 + i);
      pulse_edge(-(i % 2));
    end
    command(CmdSnapshot);
    check_stats(-1, 0, 0, 100, 119, 0);

    set_rate(-50);
    pulse_edge(3);
    pulse_edge(-5);
    pulse_edge(2);
    pulse_edge(1);
    pulse_edge(-1);
    set_fine(1'b0);
    set_fine(1'b1);
    set_fine(1'b0);
    repeat (4) @(posedge CLK);
    check_stats(-1, 0, 0, 100, 119, 0);
    command(CmdSnapshot);
    check_stats(-5, 3, 3, -50, 119, 2);
    sim_helper_bram.bram_write(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_STATS_CTL, CmdSnapshot);
    command(CmdSnapshot | CmdClear);
    command(CmdSnapshot);
    check_stats(-5, 3, 3, -50, 119, 2);

    pulse_edge(8191);
    pulse_edge(-8191);
    command(CmdSnapshot | CmdClear);
    check_stats(-8191, 8191, 5, -50, 119, 2);
    command(CmdSnapshot);
    check_stats(32767, -32768, 0, -50, -50, 0);

    pulse_edge(4);
    command(CmdClear);
    check_stats(32767, -32768, 0, -50, -50, 0);
    command(CmdSnapshot);
    check_stats(32767, -32768, 0, -50, -50, 0);

    @(negedge CLK);
    status.EDGE = 1'b1;
    status.EDGE_DIFF = 14'sd5;
    repeat (65540) @(negedge CLK);
    status.EDGE = 1'b0;
    for (int i = 0; i < 65540; i++) begin
      set_fine(1'b1);
      set_fine(1'b0);
    end
    command(CmdSnapshot);
    check_stats(5, 5, 65535, -50, -50, 65535);

    command(CmdClear);
    random_drive = 1'b1;
    live_moved   = 0;
    for (int i = 0; i < 40; i++) begin
      command((i % 2 == 0) ? CmdSnapshot : (CmdSnapshot | CmdClear));
      read_stats(got, 30);
      `ASSERT_EQ(expected.diff_min, got.diff_min);
      `ASSERT_EQ(expected.diff_max, got.diff_max);
      `ASSERT_EQ(expected.diff_outliers, got.diff_outliers);
      `ASSERT_EQ(expected.rate_min, got.rate_min);
      `ASSERT_EQ(expected.rate_max, got.rate_max);
      `ASSERT_EQ(expected.fine_exits, got.fine_exits);
      if (memory.sync_ps_regs.sync_ps_stats.diff_outliers != expected.diff_outliers) live_moved++;
    end
    random_drive = 1'b0;
    if (live_moved < 30) begin
      $error("%s:%d: accumulation while reading: expected is >= 30 snapshots overtaken, but actual is %0d", `__FILE__, `__LINE__, live_moved);
      $finish();
    end

    $display("OK! sim_sync_ps_stats");
    $finish();
  end

endmodule
