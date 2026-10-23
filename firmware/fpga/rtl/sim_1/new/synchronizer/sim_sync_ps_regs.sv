`timescale 1ns / 1ps
module sim_sync_ps_regs ();

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

  settings::sync_ps_settings_t settings;
  int updates = 0;

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
      .SYNC_PS_SETTINGS(settings)
  );

  assign cnt_bus.WE   = 1'b0;
  assign cnt_bus.ADDR = 8'd0;
  assign cnt_bus.DIN  = 16'd0;

  always @(posedge CLK) if (settings.UPDATE) updates <= updates + 1;

  task automatic reg_write(input logic [7:0] a, input logic [15:0] v);
    sim_helper_bram.bram_write(BRAM_SELECT_SYNC_PS, a, v);
  endtask

  task automatic reg_read(input logic [7:0] a, output logic [15:0] v);
    sim_helper_bram.bram_read(BRAM_SELECT_SYNC_PS, a, v);
  endtask

  logic [15:0] value;

  initial begin
    @(posedge locked);
    repeat (16) @(posedge CLK);

    `ASSERT_EQ(1'b1, settings.ENABLE);
    reg_read(ADDR_SYNC_PS_CTL, value);
    `ASSERT_EQ(16'h0001, value);

    reg_write(ADDR_SYNC_PS_CTL, 16'hFFFE);
    repeat (8) @(posedge CLK);
    `ASSERT_EQ(1'b0, settings.ENABLE);
    `ASSERT_EQ(1, updates);
    reg_read(ADDR_SYNC_PS_CTL, value);
    `ASSERT_EQ(16'h0000, value);

    updates = 0;
    reg_write(ADDR_SYNC_PS_CTL, 16'h0003);
    repeat (8) @(posedge CLK);
    `ASSERT_EQ(1'b1, settings.ENABLE);
    `ASSERT_EQ(1, updates);
    reg_read(ADDR_SYNC_PS_CTL, value);
    `ASSERT_EQ(16'h0001, value);

    reg_read(8'h01, value);
    `ASSERT_EQ(16'h0000, value);

    reg_read(ADDR_SYNC_PS_CTL, value);
    `ASSERT_EQ(16'h0001, value);
    sim_helper_bram.bram_read(BRAM_SELECT_WATCHDOG, ADDR_WATCHDOG_STATE, value);
    `ASSERT_EQ(16'd0, value);
    reg_read(ADDR_SYNC_PS_CTL, value);
    `ASSERT_EQ(16'h0001, value);

    $display("OK! sim_sync_ps_regs");
    $finish();
  end

endmodule
