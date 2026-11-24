`timescale 1ns / 1ps
module sim_sync_tdc ();

  `include "define.vh"

  import params::*;

  localparam int Taps = 512;
  localparam int Entries = 128;
  localparam real TdcPeriod = 48.828 / 10.0;

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

  logic sync_in = 1'b0;
  logic tdc_clk;
  settings::sync_tdc_ctl_t tdc_ctl;
  settings::sync_tdc_status_t tdc_status;

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
      .SYNC_PS_STATUS(),
      .SYNC_TDC_CLK(tdc_clk),
      .SYNC_TDC_CTL(tdc_ctl),
      .SYNC_TDC_STATUS(tdc_status)
  );

  sync_tdc sync_tdc (
      .CLK(CLK),
      .BUS_CLK(sim_helper_bram.memory_bus.BUS_CLK),
      .RESET(~locked),
      .SYNC_IN(sync_in),
      .CTL(tdc_ctl),
      .TDC_CLK(tdc_clk),
      .STATUS(tdc_status)
  );

  assign cnt_bus.WE   = 1'b0;
  assign cnt_bus.ADDR = 8'd0;
  assign cnt_bus.DIN  = 16'd0;

  localparam logic [15:0] CmdStart = 16'h0004;
  localparam logic [15:0] CmdCalib = 16'h0008;

  logic [15:0] ctl;
  logic [15:0] entry[Entries];
  int expected_ones[Entries];

  task automatic read_ctl();
    sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_STATS_CTL, ctl);
  endtask

  task automatic start(input logic [15:0] cmd);
    sim_helper_bram.bram_write(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_STATS_CTL, cmd);
    do begin
      @(posedge CLK);
      read_ctl();
    end while (ctl[0]);
    `ASSERT_EQ(1'b1, ctl[1]);
    repeat (4) @(posedge CLK);
  endtask

  task automatic wait_done(input int max_polls, input int jitter);
    int polls = 0;
    do begin
      @(posedge CLK);
      #($urandom_range(jitter));
      read_ctl();
      polls++;
      if (polls > max_polls) begin
        $error("%s:%d: expected is the capture finished within %0d polls, but actual is %0d entries stored (state %0d)", `__FILE__, `__LINE__,
               max_polls, sync_tdc.entry, sync_tdc.state);
        $finish();
      end
    end while (ctl[1]);
  endtask

  task automatic read_entries();
    for (int i = 0; i < Entries; i++) sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, ADDR_SYNC_PS_TDC_BUF + i, entry[i]);
  endtask

  task automatic pulse_at(input real offset);
    @(posedge CLK);
    #(offset) sync_in = 1'b1;
    repeat (2) @(posedge CLK);
    sync_in = 1'b0;
    repeat (8) @(posedge CLK);
  endtask

  function automatic logic [Taps-1:0] thermometer(input int ones, input int bubble);
    logic [Taps-1:0] v = '0;
    for (int i = 0; i < Taps; i++) v[i] = i < ones;
    if (bubble == 1 && ones > 2) v[ones-2] = 1'b0;
    if (bubble == 2 && ones < Taps - 2) v[ones+1] = 1'b1;
    if (bubble == 3 && ones > 2 && ones < Taps - 2) begin
      v[ones-2] = 1'b0;
      v[ones+1] = 1'b1;
    end
    return v;
  endfunction

  function automatic int popcount(input logic [Taps-1:0] v);
    int n = 0;
    for (int i = 0; i < Taps; i++) n += v[i];
    return n;
  endfunction

  int first_slot;
  logic [Taps-1:0] pattern;
  logic [Taps-1:0] later;
  int calib_slots[10];
  int calib_distinct;

  initial begin
    @(posedge locked);
    do begin
      repeat (16) @(posedge CLK);
      read_ctl();
    end while (!ctl[2]);
    `ASSERT_EQ(1'b0, ctl[1]);
    `ASSERT_EQ(1'b0, ctl[0]);

    start(CmdStart);
    for (int i = 0; i < Entries; i++) pulse_at(((i % 10) + 0.5) * TdcPeriod);
    wait_done(16, 0);
    read_entries();
    first_slot = int'(entry[0][13:10]);
    for (int i = 0; i < Entries; i++) begin
      `ASSERT_EQ(Taps, int'(entry[i][9:0]));
      `ASSERT_EQ((first_slot + i) % 10, int'(entry[i][13:10]));
      `ASSERT_EQ(2'b00, entry[i][15:14]);
    end

    start(CmdStart);
    for (int i = 0; i < Entries; i++) begin
      pattern = thermometer(1 + $urandom_range(Taps - 1), $urandom_range(3));
      expected_ones[i] = popcount(pattern);
      later = thermometer(1 + $urandom_range(Taps - 1), 0);
      @(posedge tdc_clk);
      force sync_tdc.taps = pattern;
      @(posedge tdc_clk);
      force sync_tdc.taps = later;
      repeat (8) @(posedge tdc_clk);
      force sync_tdc.taps = '0;
      repeat (60) @(posedge tdc_clk);
    end
    release sync_tdc.taps;
    wait_done(16, 0);
    read_entries();
    for (int i = 0; i < Entries; i++) `ASSERT_EQ(expected_ones[i], int'(entry[i][9:0]));

    start(CmdStart);
    start(CmdStart);
    for (int i = 0; i < Entries; i++) pulse_at(0.5 * TdcPeriod);
    wait_done(16, 0);
    read_entries();
    for (int i = 0; i < Entries; i++) `ASSERT_EQ(first_slot, int'(entry[i][13:10]));

    start(CmdStart);
    repeat (100) @(posedge CLK);
    read_ctl();
    `ASSERT_EQ(1'b1, ctl[1]);
    start(CmdStart | CmdCalib);
    wait_done(100000, 48);
    `ASSERT_EQ(1'b0, sync_tdc.source);
    read_entries();
    for (int i = 0; i < 10; i++) calib_slots[i] = 0;
    for (int i = 0; i < Entries; i++) begin
      `ASSERT_EQ(Taps, int'(entry[i][9:0]));
      calib_slots[entry[i][13:10]]++;
    end
    calib_distinct = 0;
    for (int i = 0; i < 10; i++) if (calib_slots[i] > 0) calib_distinct++;
    if (calib_distinct < 2) begin
      $error("%s:%d: expected is the calibration source hits >= 2 slots, but actual is %0d", `__FILE__, `__LINE__, calib_distinct);
      $finish();
    end

    $display("OK! sim_sync_tdc");
    $finish();
  end

endmodule
