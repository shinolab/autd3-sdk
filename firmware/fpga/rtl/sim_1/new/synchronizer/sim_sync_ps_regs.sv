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
  settings::sync_ps_state_t state;
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
      .SYNC_PS_SETTINGS(settings),
      .SYNC_PS_STATE(state)
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
    state.FINE = 1'b0;
    state.FINE_EXIT_COUNT = 8'd0;
    state.RATE = 16'sd0;
    state.INTEG = 16'sd0;
    state.PD_POS = 16'd0;
    state.PD_NEG = 16'd0;
    state.EDGE_DIFF = 16'sd0;
    state.EDGE_COUNT = 16'd0;

    @(posedge locked);
    repeat (16) @(posedge CLK);

    `ASSERT_EQ(1'b1, settings.ENABLE);
    `ASSERT_EQ(1'b0, settings.MANUAL);
    `ASSERT_EQ(16'd64, settings.KP);
    `ASSERT_EQ(16'd1024, settings.KI);
    `ASSERT_EQ(16'd4096, settings.KF);
    `ASSERT_EQ(16'sd0, settings.MANUAL_RATE);
    reg_read(ADDR_SYNC_PS_KI, value);
    `ASSERT_EQ(16'd1024, value);

    reg_write(ADDR_SYNC_PS_KP, 16'd11);
    reg_write(ADDR_SYNC_PS_KI, 16'd22);
    reg_write(ADDR_SYNC_PS_KF, 16'd33);
    reg_write(ADDR_SYNC_PS_MANUAL_RATE, 16'hFF85);
    reg_write(ADDR_SYNC_PS_CTL, 16'h0003);
    repeat (8) @(posedge CLK);
    `ASSERT_EQ(1'b1, settings.ENABLE);
    `ASSERT_EQ(1'b1, settings.MANUAL);
    `ASSERT_EQ(16'd11, settings.KP);
    `ASSERT_EQ(16'd22, settings.KI);
    `ASSERT_EQ(16'd33, settings.KF);
    `ASSERT_EQ(-16'sd123, settings.MANUAL_RATE);
    `ASSERT_EQ(1'b1, updates > 0);

    reg_read(ADDR_SYNC_PS_CTL, value);
    `ASSERT_EQ(16'h0003, value);
    reg_read(ADDR_SYNC_PS_MANUAL_RATE, value);
    `ASSERT_EQ(16'hFF85, value);

    updates = 0;
    reg_write(ADDR_SYNC_PS_CTL, 16'h0001);
    repeat (8) @(posedge CLK);
    `ASSERT_EQ(1'b1, settings.ENABLE);
    `ASSERT_EQ(1'b0, settings.MANUAL);
    `ASSERT_EQ(1, updates);

    state.FINE = 1'b1;
    state.FINE_EXIT_COUNT = 8'd5;
    state.RATE = -16'sd1234;
    state.INTEG = 16'sd4321;
    state.PD_POS = 16'd111;
    state.PD_NEG = 16'd222;
    state.EDGE_DIFF = -16'sd1;
    state.EDGE_COUNT = 16'd333;
    sim_helper_bram.bus_active = 1'b1;
    repeat (2100) @(posedge CLK);
    sim_helper_bram.bus_active = 1'b0;

    reg_read(ADDR_SYNC_PS_STATE, value);
    `ASSERT_EQ(16'h0501, value);
    reg_read(ADDR_SYNC_PS_RATE, value);
    `ASSERT_EQ(-16'sd1234, $signed(value));
    reg_read(ADDR_SYNC_PS_INTEG, value);
    `ASSERT_EQ(16'sd4321, $signed(value));
    reg_read(ADDR_SYNC_PS_PD_POS, value);
    `ASSERT_EQ(16'd111, value);
    reg_read(ADDR_SYNC_PS_PD_NEG, value);
    `ASSERT_EQ(16'd222, value);
    reg_read(ADDR_SYNC_PS_EDGE_DIFF, value);
    `ASSERT_EQ(16'hFFFF, value);
    reg_read(ADDR_SYNC_PS_EDGE_COUNT, value);
    `ASSERT_EQ(16'd333, value);

    sim_helper_bram.bram_read(BRAM_SELECT_WATCHDOG, ADDR_WATCHDOG_TIMEOUT, value);
    `ASSERT_EQ(16'd0, value);
    reg_read(ADDR_SYNC_PS_EDGE_COUNT, value);
    `ASSERT_EQ(16'd333, value);

    $display("OK! sim_sync_ps_regs");
    $finish();
  end

endmodule
