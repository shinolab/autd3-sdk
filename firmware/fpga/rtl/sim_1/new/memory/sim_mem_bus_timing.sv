`timescale 1ns / 1ps
module sim_mem_bus_timing ();

  `include "define.vh"

  import params::*;

  localparam real TCK = 13.334;
  localparam real SKEW = 0.05;
  localparam real T_RDS = 4.0;
  localparam int SIZE = 256;

  logic CLK;
  logic locked;
  logic CPU_CKIO = 1'b0;

  sim_helper_random sim_helper_random ();

  sim_helper_clk sim_helper_clk (
      .CLK(CLK),
      .LOCKED(locked),
      .SYS_TIME()
  );

  memory_bus_if memory_bus ();
  cnt_bus_if cnt_bus ();
  phase_corr_bus_if phase_corr_bus ();
  modulation_bus_if mod_bus ();
  emission_bus_if emission_bus ();
  pwe_table_bus_if pwe_table_bus ();
  flash_bus_if flash_bus ();
  output_mask_bus_if output_mask_bus ();

  memory memory (
      .CLK(CLK),
      .MEM_BUS(memory_bus.bram_port),
      .CNT_BUS(cnt_bus.in_port),
      .PHASE_CORR_BUS(phase_corr_bus.in_port),
      .OUTPUT_MASK_BUS(output_mask_bus.in_port),
      .MOD_BUS(mod_bus.in_port),
      .EMISSION_BUS(emission_bus.in_port),
      .PWE_TABLE_BUS(pwe_table_bus.in_port),
      .FLASH_BUS(flash_bus.host_port)
  );

  assign cnt_bus.WE   = 1'b0;
  assign cnt_bus.ADDR = '0;
  assign cnt_bus.DIN  = '0;

  logic cs_n = 1'b1;
  logic we_n = 1'b1;
  logic rd_n = 1'b1;
  logic rdwr = 1'b0;
  logic [15:0] bus_addr = '0;
  logic [15:0] wdata = '0;

  assign memory_bus.BUS_CLK = CPU_CKIO;
  assign memory_bus.CS_N = cs_n;
  assign memory_bus.WE_N = we_n;
  assign memory_bus.RD_N = rd_n;
  assign memory_bus.RDWR = rdwr;
  assign memory_bus.BRAM_ADDR = bus_addr;
  assign memory_bus.DATA_IN = wdata;

  logic [15:0] expected[SIZE];
  int errors = 0;

  initial begin
    forever #(TCK / 2) CPU_CKIO = ~CPU_CKIO;
  end

  function automatic logic [15:0] ctl_addr(input logic [7:0] idx);
    return {BRAM_SELECT_CONTROLLER, idx};
  endfunction

  task automatic align();
    @(posedge CPU_CKIO);
    #(TCK - SKEW);
  endtask

  task automatic bus_write(input logic [15:0] addr, input logic [15:0] data, input logic [15:0] next_addr, input logic [15:0] next_data);
    cs_n = 1'b0;
    rdwr = 1'b0;
    bus_addr = addr;
    wdata = data;
    #(2 * TCK + 2 * SKEW);
    we_n = 1'b0;
    #(4 * TCK - 2 * SKEW);
    bus_addr = next_addr;
    wdata = next_data;
    #(2 * SKEW);
    we_n = 1'b1;
    cs_n = 1'b1;
    #(TCK - 2 * SKEW);
  endtask

  task automatic bus_read(input logic [15:0] addr, input logic [15:0] next_addr, output logic [15:0] data);
    rdwr = 1'b1;
    bus_addr = addr;
    cs_n = 1'b0;
    #(2 * TCK + 2 * SKEW);
    rd_n = 1'b0;
    #(3.5 * TCK - SKEW - T_RDS);
    data = memory_bus.CPU_DATA;
    #(0.5 * TCK + T_RDS - SKEW);
    bus_addr = next_addr;
    #(2 * SKEW);
    rd_n = 1'b1;
    cs_n = 1'b1;
    rdwr = 1'b0;
    #(TCK - 2 * SKEW);
  endtask

  task automatic check(input logic [7:0] idx);
    logic [15:0] value;
    bus_read(ctl_addr(idx), ctl_addr(idx ^ 8'h55), value);
    if (value !== expected[idx]) begin
      $display("ERR: addr=%0d expected=%h got=%h", idx, expected[idx], value);
      errors++;
    end
  endtask

  initial begin
    @(posedge locked);

    for (int i = 0; i < SIZE; i++) begin
      expected[i] = sim_helper_random.range(16'hFFFF, 0);
    end

    align();
    for (int i = 0; i < SIZE; i++) begin
      bus_write(ctl_addr(i[7:0]), expected[i], ctl_addr(i[7:0] ^ 8'h55), ~expected[i]);
      check(i[7:0]);
      if ((i[7:0] ^ 8'h55) < i[7:0]) begin
        check(i[7:0] ^ 8'h55);
      end
    end
    $display("memory initialized");

    for (int i = 0; i < SIZE; i++) begin
      check(i[7:0]);
    end

    `ASSERT_EQ(0, errors);

    $display("OK! sim_mem_bus_timing");
    $finish();
  end

endmodule
