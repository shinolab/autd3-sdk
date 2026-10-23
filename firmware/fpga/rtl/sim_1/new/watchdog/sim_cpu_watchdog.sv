`timescale 1ns / 1ps
module sim_cpu_watchdog ();

  `include "define.vh"

  import params::*;

  localparam realtime MS = 1_000_000;

  logic CLK;
  logic locked;
  logic fired;

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
      .CPU_WATCHDOG_FIRED(fired)
  );

  assign cnt_bus.WE   = 1'b0;
  assign cnt_bus.ADDR = 8'd0;
  assign cnt_bus.DIN  = 16'd0;

  task automatic reg_write(input logic [7:0] a, input logic [15:0] v);
    sim_helper_bram.bram_write(BRAM_SELECT_WATCHDOG, a, v);
  endtask

  task automatic reg_read(input logic [7:0] a, output logic [15:0] v);
    sim_helper_bram.bram_read(BRAM_SELECT_WATCHDOG, a, v);
  endtask

  task automatic kick();
    reg_write(ADDR_WATCHDOG_KICK, 16'd0);
  endtask

  task automatic set_timeout(input logic [15:0] ms);
    reg_write(ADDR_WATCHDOG_TIMEOUT, ms);
  endtask

  task automatic release_gate();
    reg_write(ADDR_WATCHDOG_RELEASE, 16'd0);
    repeat (8) @(posedge CLK);
    `ASSERT_EQ(1'b0, fired);
  endtask

  task automatic expect_state(input logic expected_fired, input logic [7:0] expected_count);
    logic [15:0] value;
    reg_read(ADDR_WATCHDOG_STATE, value);
    `ASSERT_EQ(expected_fired, value[0]);
    `ASSERT_EQ(7'd0, value[7:1]);
    `ASSERT_EQ(expected_count, value[15:8]);
  endtask

  task automatic expect_timeout_reads_zero();
    logic [15:0] value;
    reg_read(ADDR_WATCHDOG_TIMEOUT, value);
    `ASSERT_EQ(16'd0, value);
  endtask

  task automatic expect_quiet_for(input realtime duration);
    realtime deadline;
    deadline = $realtime + duration;
    while ($realtime < deadline) begin
      @(posedge CLK);
      `ASSERT_EQ(1'b0, fired);
    end
  endtask

  task automatic expect_fire_between(input realtime earliest, input realtime latest);
    realtime start;
    start = $realtime;
    `ASSERT_EQ(1'b0, fired);
    while (!fired && ($realtime - start <= latest)) begin
      @(posedge CLK);
    end
    `ASSERT_EQ(1'b1, fired);
    `ASSERT_EQ(1'b1, ($realtime - start) >= earliest);
  endtask

  initial begin
    @(posedge locked);
    repeat (16) @(posedge CLK);

    expect_state(1'b0, 8'd0);
    expect_quiet_for(5 * MS);
    expect_state(1'b0, 8'd0);

    set_timeout(16'd2);
    expect_timeout_reads_zero();
    for (int i = 0; i < 10; i++) begin
      expect_quiet_for(1 * MS);
      kick();
    end
    expect_state(1'b0, 8'd0);

    expect_fire_between(1.99 * MS, 3.1 * MS);
    expect_state(1'b1, 8'd1);

    kick();
    repeat (8) @(posedge CLK);
    `ASSERT_EQ(1'b1, fired);
    set_timeout(16'd0);
    repeat (8) @(posedge CLK);
    `ASSERT_EQ(1'b1, fired);
    expect_state(1'b1, 8'd1);

    release_gate();
    expect_state(1'b0, 8'd1);
    expect_quiet_for(5 * MS);

    set_timeout(16'd3);
    expect_quiet_for(2.9 * MS);
    set_timeout(16'd3);
    expect_fire_between(2.99 * MS, 4.1 * MS);
    expect_state(1'b1, 8'd2);

    set_timeout(16'd1);
    release_gate();
    expect_fire_between(0.99 * MS, 2.1 * MS);
    expect_state(1'b1, 8'd3);

    for (int n = 4; n <= 260; n++) begin
      release_gate();
      expect_fire_between(0.99 * MS, 2.1 * MS);
    end
    expect_state(1'b1, 8'd255);

    release_gate();
    expect_state(1'b0, 8'd255);

    $display("OK! sim_cpu_watchdog");
    $finish();
  end

endmodule
